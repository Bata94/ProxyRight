{
  rustPlatform,
  openssl,
  pkg-config,
}:
rustPlatform.buildRustPackage {
  name = "echoserver";
  src = ./.;
  buildInputs = [openssl];
  nativeBuildInputs = [pkg-config];
  cargoLock.lockFile = ./Cargo.lock;
}
