{ lib
, rustPlatform
, pkg-config
}:

rustPlatform.buildRustPackage {
  pname = "mcserman";
  version = "0.1.0";

  src = lib.cleanSource ../.;

  cargoLock = {
    lockFile = ../Cargo.lock;
  };

  meta = with lib; {
    description = "A powerful, host-independent Minecraft Dedicated Server Manager in Rust";
    homepage = "https://github.com/AdrianChaSan/mcserman";
    license = licenses.mit;
    mainProgram = "mcserman";
    platforms = platforms.linux;
  };
}
