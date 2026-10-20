{ lib
, rustPlatform
, makeWrapper
, temurin-bin-21
, temurin-bin-17
, temurin-bin-8
}:

rustPlatform.buildRustPackage {
  pname = "mcserman";
  version = "0.1.0";

  src = lib.cleanSource ../.;

  cargoLock = {
    lockFile = ../Cargo.lock;
  };

  nativeBuildInputs = [ makeWrapper ];

  postInstall = ''
    wrapProgram $out/bin/mcserman \
      --prefix PATH : ${lib.makeBinPath [ temurin-bin-21 temurin-bin-17 temurin-bin-8 ]} \
      --set-default MCSERVER_JAVA_21_PATH "${temurin-bin-21}/bin/java" \
      --set-default MCSERVER_JAVA_17_PATH "${temurin-bin-17}/bin/java" \
      --set-default MCSERVER_JAVA_8_PATH "${temurin-bin-8}/bin/java"
  '';

  meta = with lib; {
    description = "A powerful, host-independent Minecraft Dedicated Server Manager in Rust";
    homepage = "https://github.com/AdrianChaSan/mcserman";
    license = licenses.mit;
    mainProgram = "mcserman";
    platforms = platforms.linux;
  };
}
