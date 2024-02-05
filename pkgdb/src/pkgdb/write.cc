/* ========================================================================== *
 *
 * @file pkgdb/write.cc
 *
 * @brief Implementations for writing to a SQLite3 package set database.
 *
 *
 * -------------------------------------------------------------------------- */

#include <limits>
#include <memory>

#include "flox/flake-package.hh"
#include "flox/pkgdb/write.hh"

#include "./schemas.hh"


/* -------------------------------------------------------------------------- */

namespace flox::pkgdb {

/* -------------------------------------------------------------------------- */

/** @brief Create views in database if they do not exist. */
static void
initViews( SQLiteDb & pdb )
{
  sqlite3pp::command cmd( pdb, sql_views );
  if ( sql_rc rcode = cmd.execute_all(); isSQLError( rcode ) )
    {
      throw PkgDbException( nix::fmt( "failed to initialize views:(%d) %s",
                                      rcode,
                                      pdb.error_msg() ) );
    }
}


/* -------------------------------------------------------------------------- */

/**
 * @brief Update the database's `VIEW`s schemas.
 *
 * This deletes any existing `VIEW`s and recreates them, and updates the
 * `DbVersions` row for `pkgdb_views_schema`.
 */
static void
updateViews( SQLiteDb & pdb )
{
  /* Drop all existing views. */
  {
    sqlite3pp::query qry( pdb,
                          "SELECT name FROM sqlite_master WHERE"
                          " ( type = 'view' )" );
    for ( auto row : qry )
      {
        auto               name = row.get<std::string>( 0 );
        std::string        cmd  = "DROP VIEW IF EXISTS '" + name + '\'';
        sqlite3pp::command dropView( pdb, cmd.c_str() );
        if ( sql_rc rcode = dropView.execute(); isSQLError( rcode ) )
          {
            throw PkgDbException( nix::fmt( "failed to drop view '%s':(%d) %s",
                                            name,
                                            rcode,
                                            pdb.error_msg() ) );
          }
      }
  }

  /* Update the `pkgdb_views_schema' version. */
  sqlite3pp::command updateVersion(
    pdb,
    "UPDATE DbVersions SET version = ? WHERE name = 'pkgdb_views_schema'" );
  updateVersion.bind( 1, static_cast<int>( sqlVersions.views ) );

  if ( sql_rc rcode = updateVersion.execute_all(); isSQLError( rcode ) )
    {
      throw PkgDbException( nix::fmt( "failed to update PkgDb Views:(%d) %s",
                                      rcode,
                                      pdb.error_msg() ) );
    }

  /* Redefine the `VIEW's */
  initViews( pdb );
}


/* -------------------------------------------------------------------------- */

/** @brief Create tables in database if they do not exist. */
static void
initTables( SQLiteDb & pdb )
{
  sqlite3pp::command cmdVersions( pdb, sql_versions );
  if ( sql_rc rcode = cmdVersions.execute(); isSQLError( rcode ) )
    {
      throw PkgDbException(
        nix::fmt( "failed to initialize DbVersions table:(%d) %s",
                  rcode,
                  pdb.error_msg() ) );
    }

  sqlite3pp::command cmdInput( pdb, sql_input );
  if ( sql_rc rcode = cmdInput.execute_all(); isSQLError( rcode ) )
    {
      throw PkgDbException(
        nix::fmt( "failed to initialize LockedFlake table:(%d) %s",
                  rcode,
                  pdb.error_msg() ) );
    }

  sqlite3pp::command cmdAttrSets( pdb, sql_attrSets );
  if ( sql_rc rcode = cmdAttrSets.execute_all(); isSQLError( rcode ) )
    {
      throw PkgDbException(
        nix::fmt( "failed to initialize AttrSets table:(%d) %s",
                  rcode,
                  pdb.error_msg() ) );
    }

  sqlite3pp::command cmdPackages( pdb, sql_packages );
  if ( sql_rc rcode = cmdPackages.execute_all(); isSQLError( rcode ) )
    {
      throw PkgDbException(
        nix::fmt( "failed to initialize Packages table:(%d) %s",
                  rcode,
                  pdb.error_msg() ) );
    }
}


/* -------------------------------------------------------------------------- */

/** @brief Create `DbVersions` rows if they do not exist. */
static void
initVersions( SQLiteDb & pdb )
{
  sqlite3pp::command defineVersions(
    pdb,
    "INSERT OR IGNORE INTO DbVersions ( name, version ) VALUES"
    "  ( 'pkgdb',        '" FLOX_PKGDB_VERSION "' )"
    ", ( 'pkgdb_tables_schema', ? )"
    ", ( 'pkgdb_views_schema', ? )" );
  defineVersions.bind( 1, static_cast<int>( sqlVersions.tables ) );
  defineVersions.bind( 2, static_cast<int>( sqlVersions.views ) );
  if ( sql_rc rcode = defineVersions.execute(); isSQLError( rcode ) )
    {
      throw PkgDbException( "failed to write DbVersions info",
                            pdb.error_msg() );
    }
}


/* -------------------------------------------------------------------------- */

void
PkgDb::init()
{
  initTables( this->db );
  initVersions( this->db );

  /* If the views version is outdated, update them. */
  if ( this->getDbVersion().views < sqlVersions.views )
    {
      updateViews( this->db );
    }
  else { initViews( this->db ); }
}


/* -------------------------------------------------------------------------- */

/**
 * @brief Write @a this `PkgDb` `lockedRef` and `fingerprint` fields to
 *        database metadata.
 */
static void
writeInput( PkgDb & pdb )
{
  sqlite3pp::command cmd(
    pdb.db,
    "INSERT OR IGNORE INTO LockedFlake ( fingerprint, string, attrs ) VALUES"
    "  ( ?, ?, ? )" );
  std::string fpStr = pdb.fingerprint.to_string( nix::Base16, false );
  cmd.bind( 1, fpStr, sqlite3pp::copy );
  cmd.bind( 2, pdb.lockedRef.string, sqlite3pp::nocopy );
  cmd.bind( 3, pdb.lockedRef.attrs.dump(), sqlite3pp::copy );
  if ( sql_rc rcode = cmd.execute(); isSQLError( rcode ) )
    {
      throw PkgDbException( "failed to write LockedFlaked info",
                            pdb.db.error_msg() );
    }
}


/* -------------------------------------------------------------------------- */

PkgDb::PkgDb( const nix::flake::LockedFlake & flake, std::string_view dbPath )
{
  this->dbPath      = dbPath;
  this->fingerprint = flake.getFingerprint();
  this->connect();
  this->init();
  this->lockedRef
    = { flake.flake.lockedRef.to_string(),
        nix::fetchers::attrsToJSON( flake.flake.lockedRef.toAttrs() ) };
  writeInput( *this );
}


/* -------------------------------------------------------------------------- */

void
PkgDb::connect()
{
  this->db.connect( this->dbPath.string().c_str(),
                    SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE );
}


/* -------------------------------------------------------------------------- */

row_id
PkgDb::addOrGetAttrSetId( const std::string & attrName, row_id parent )
{
  sqlite3pp::command cmd(
    this->db,
    "INSERT INTO AttrSets ( attrName, parent ) VALUES ( ?, ? )" );
  cmd.bind( 1, attrName, sqlite3pp::copy );
  cmd.bind( 2, static_cast<long long>( parent ) );
  if ( sql_rc rcode = cmd.execute(); isSQLError( rcode ) )
    {
      sqlite3pp::query qryId(
        this->db,
        "SELECT id FROM AttrSets WHERE ( attrName = ? ) AND ( parent = ? )" );
      qryId.bind( 1, attrName, sqlite3pp::copy );
      qryId.bind( 2, static_cast<long long>( parent ) );
      auto row = qryId.begin();
      if ( row == qryId.end() )
        {
          throw PkgDbException(
            nix::fmt( "failed to add AttrSet.id `AttrSets[%ull].%s':(%d) %s",
                      parent,
                      attrName,
                      rcode,
                      this->db.error_msg() ) );
        }
      return ( *row ).get<long long>( 0 );
    }
  return this->db.last_insert_rowid();
}


/* -------------------------------------------------------------------------- */

row_id
PkgDb::addOrGetAttrSetId( const flox::AttrPath & path )
{
  row_id row = 0;
  for ( const auto & attr : path ) { row = addOrGetAttrSetId( attr, row ); }
  return row;
}


/* -------------------------------------------------------------------------- */

row_id
PkgDb::addOrGetDescriptionId( const std::string & description )
{
  sqlite3pp::query qry(
    this->db,
    "SELECT id FROM Descriptions WHERE description = ? LIMIT 1" );
  qry.bind( 1, description, sqlite3pp::copy );
  auto rows = qry.begin();
  if ( rows != qry.end() )
    {
      nix::Activity act(
        *nix::logger,
        nix::lvlDebug,
        nix::actUnknown,
        nix::fmt( "Found existing description in database: %s.",
                  description ) );
      return ( *rows ).get<long long>( 0 );
    }

  sqlite3pp::command cmd(
    this->db,
    "INSERT INTO Descriptions ( description ) VALUES ( ? )" );
  cmd.bind( 1, description, sqlite3pp::copy );
  nix::Activity act(
    *nix::logger,
    nix::lvlDebug,
    nix::actUnknown,
    nix::fmt( "Adding new description to database: %s.", description ) );
  if ( sql_rc rcode = cmd.execute(); isSQLError( rcode ) )
    {
      throw PkgDbException( nix::fmt( "failed to add Description '%s':(%d) %s",
                                      description,
                                      rcode,
                                      this->db.error_msg() ) );
    }
  return this->db.last_insert_rowid();
}


/* -------------------------------------------------------------------------- */

row_id
PkgDb::addPackage( row_id               parentId,
                   std::string_view     attrName,
                   const flox::Cursor & cursor,
                   bool                 replace,
                   bool                 checkDrv )
{
#define ADD_PKG_BODY                                                   \
  " INTO Packages ("                                                   \
  "  parentId, attrName, name, pname, version, semver, license"        \
  ", outputs, outputsToInstall, broken, unfree, descriptionId"         \
  ") VALUES ("                                                         \
  "  :parentId, :attrName, :name, :pname, :version, :semver, :license" \
  ", :outputs, :outputsToInstall, :broken, :unfree, :descriptionId"    \
  ")"
  static const char * qryIgnore  = "INSERT OR IGNORE" ADD_PKG_BODY;
  static const char * qryReplace = "INSERT OR REPLACE" ADD_PKG_BODY;

  sqlite3pp::command cmd( this->db, replace ? qryReplace : qryIgnore );

  /* We don't need to reference any `attrPath' related info here, so
   * we can avoid looking up the parent path by passing a phony one to the
   * `FlakePackage' constructor here. */
  FlakePackage pkg( cursor, { "packages", "x86_64-linux", "phony" }, checkDrv );
  std::string  attrNameS( attrName );

  cmd.bind( ":parentId", static_cast<long long>( parentId ) );
  cmd.bind( ":attrName", attrNameS, sqlite3pp::copy );
  cmd.bind( ":name", pkg._fullName, sqlite3pp::nocopy );
  cmd.bind( ":pname", pkg._pname, sqlite3pp::nocopy );

  if ( pkg._version.empty() ) { cmd.bind( ":version" ); /* bind NULL */ }
  else { cmd.bind( ":version", pkg._version, sqlite3pp::nocopy ); }

  if ( pkg._semver.has_value() )
    {
      cmd.bind( ":semver", *pkg._semver, sqlite3pp::nocopy );
    }
  else { cmd.bind( ":semver" ); /* binds NULL */ }

  {
    nlohmann::json jOutputs = pkg.getOutputs();
    cmd.bind( ":outputs", jOutputs.dump(), sqlite3pp::copy );
  }
  {
    nlohmann::json jOutsInstall = pkg.getOutputsToInstall();
    cmd.bind( ":outputsToInstall", jOutsInstall.dump(), sqlite3pp::copy );
  }


  if ( pkg._hasMetaAttr )
    {
      if ( auto maybe = pkg.getLicense(); maybe.has_value() )
        {
          cmd.bind( ":license", *maybe, sqlite3pp::copy );
        }
      else { cmd.bind( ":license" ); }

      if ( auto maybe = pkg.isBroken(); maybe.has_value() )
        {
          cmd.bind( ":broken", static_cast<int>( *maybe ) );
        }
      else { cmd.bind( ":broken" ); }

      if ( auto maybe = pkg.isUnfree(); maybe.has_value() )
        {
          cmd.bind( ":unfree", static_cast<int>( *maybe ) );
        }
      else /* TODO: Derive value from `license'? */ { cmd.bind( ":unfree" ); }

      if ( auto maybe = pkg.getDescription(); maybe.has_value() )
        {
          row_id descriptionId = this->addOrGetDescriptionId( *maybe );
          cmd.bind( ":descriptionId", static_cast<long long>( descriptionId ) );
        }
      else { cmd.bind( ":descriptionId" ); }
    }
  else
    {
      /* binds NULL */
      cmd.bind( ":license" );
      cmd.bind( ":broken" );
      cmd.bind( ":unfree" );
      cmd.bind( ":descriptionId" );
    }
  if ( sql_rc rcode = cmd.execute(); isSQLError( rcode ) )
    {
      throw PkgDbException(
        nix::fmt( "failed to write Package '%s'", pkg._fullName ),
        this->db.error_msg() );
    }
  return this->db.last_insert_rowid();
}


/* -------------------------------------------------------------------------- */

void
PkgDb::setPrefixDone( row_id prefixId, bool done )
{
  sqlite3pp::command cmd( this->db, R"SQL(
    UPDATE AttrSets SET done = ? WHERE id in (
      WITH RECURSIVE Tree AS (
        SELECT id, parent, 0 as depth FROM AttrSets
        WHERE ( id = ? )
        UNION ALL SELECT O.id, O.parent, ( Parent.depth + 1 ) AS depth
        FROM AttrSets O
        JOIN Tree AS Parent ON ( Parent.id = O.parent )
      ) SELECT C.id FROM Tree AS C
      JOIN AttrSets AS Parent ON ( C.parent = Parent.id )
    )
  )SQL" );
  cmd.bind( 1, static_cast<int>( done ) );
  cmd.bind( 2, static_cast<long long>( prefixId ) );
  if ( sql_rc rcode = cmd.execute(); isSQLError( rcode ) )
    {
      throw PkgDbException(
        nix::fmt( "failed to set AttrSets.done for subtree '%s':(%d) %s",
                  concatStringsSep( ".", this->getAttrSetPath( prefixId ) ),
                  rcode,
                  this->db.error_msg() ) );
    }
}

void
PkgDb::setPrefixDone( const flox::AttrPath & prefix, bool done )
{
  this->setPrefixDone( this->addOrGetAttrSetId( prefix ), done );
}


/* -------------------------------------------------------------------------- */

/* NOTE:
 * Benchmarks on large catalogs have indicated that using a _todo_ queue instead
 * of recursion is faster and consumes less memory.
 * Repeated runs against `nixpkgs-flox` come in at ~2m03s using recursion and
 * ~1m40s using a queue. */
void
PkgDb::scrape( nix::SymbolTable & syms, const Target & target, Todos & todo )
{
  const auto & [prefix, cursor, parentId] = target;

  /* If it has previously been scraped then bail out. */
  if ( this->completedAttrSet( parentId ) ) { return; }

  bool tryRecur = prefix.front() != "packages";

  debugLog( nix::fmt( "evaluating package set '%s'",
                      concatStringsSep( ".", prefix ) ) );

  /* Scrape loop over attrs */
  for ( nix::Symbol & aname : cursor->getAttrs() )
    {
      if ( syms[aname] == "recurseForDerivations" ) { continue; }

      /* Used for logging, but can skip it at low verbosity levels. */
      const std::string pathS
        = ( nix::lvlTalkative <= nix::verbosity )
            ? concatStringsSep( ".", prefix ) + "." + syms[aname]
            : "";

      traceLog( "\tevaluating attribute '" + pathS + "'" );

      try
        {
          flox::Cursor child = cursor->getAttr( aname );
          if ( child->isDerivation() )
            {
              this->addPackage( parentId, syms[aname], child );
              continue;
            }
          if ( ! tryRecur ) { continue; }
          if ( auto maybe = child->maybeGetAttr( "recurseForDerivations" );
               ( ( maybe != nullptr ) && maybe->getBool() )
               /* XXX: We explicitly recurse into `legacyPackages.*.darwin'
                *      due to a bug in `nixpkgs' which doesn't set
                *      `recurseForDerivations' attribute correctly. */
               || ( ( prefix.front() == "legacyPackages" )
                    && ( syms[aname] == "darwin" ) ) )
            {
              flox::AttrPath path = prefix;
              path.emplace_back( syms[aname] );
              if ( nix::lvlTalkative <= nix::verbosity )
                {
                  nix::logger->log( nix::lvlTalkative,
                                    "\tpushing target '" + pathS + "'" );
                }
              row_id childId = this->addOrGetAttrSetId( syms[aname], parentId );
              todo.emplace( std::make_tuple( std::move( path ),
                                             std::move( child ),
                                             childId ) );
            }
        }
      catch ( const nix::EvalError & err )
        {
          /* Ignore errors in `legacyPackages' */
          if ( prefix.front() == "legacyPackages" )
            {
              /* Only print eval errors in "debug" mode. */
              nix::ignoreException( nix::lvlDebug );
            }
          else { throw; }
        }
      catch ( const std::bad_alloc & err )
        {
          /* We need to try this attribute set again in a sibling process. */
          debugLog( nix::fmt( "ran out of memory evaluating attribute '%s'."
                              " will try again in a sibling process.",
                              pathS ) );

          throw;
        }
    }
}


/* -------------------------------------------------------------------------- */

}  // namespace flox::pkgdb


/* -------------------------------------------------------------------------- *
 *
 *
 *
 * ========================================================================== */
