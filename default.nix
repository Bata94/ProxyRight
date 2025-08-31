{
  rustPlatform,
  openssl,
  pkg-config,
}:
rustPlatform.buildRustPackage {
  name = "ProxyRight";
  src = ./.;
  buildInputs = [openssl];
  nativeBuildInputs = [pkg-config];
  cargoLock.lockFile = ./Cargo.lock;
}
