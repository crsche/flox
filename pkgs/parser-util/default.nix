{
  inputs,
  callPackage,
  flox-nix,
  ...
}:
callPackage (inputs.parser-util + "/pkg-fun.nix") {
  nix = flox-nix;
}
