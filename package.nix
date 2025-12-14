{
  android-tools,
  clang,
  expat,
  fontconfig,
  freetype,
  lib,
  libglvnd,
  libxkbcommon,
  makeWrapper,
  mold,
  pkg-config,
  rustPlatform,
  stdenv,
  wayland,
  writableTmpDirAsHomeHook,
  xorg,
}:

rustPlatform.buildRustPackage (
  finalAttrs:
  let
    pname = "universal-android-debloater";

    linuxRuntimeLibs = [
      fontconfig
      freetype
      libglvnd
      libxkbcommon
      wayland
      xorg.libX11
      xorg.libXcursor
      xorg.libXi
      xorg.libXrandr
    ];

    darwinRuntimeLibs = [
      fontconfig
      freetype
    ];
  in
  {
    inherit pname;
    version = "1.2.0-pre";

    src = builtins.path {
      path = ./.;
      name = "source";
    };

    cargoLock.lockFile = ./Cargo.lock;

    buildInputs =
      lib.optionals stdenv.hostPlatform.isLinux (
        [
          expat
          fontconfig
          freetype
        ]
        ++ linuxRuntimeLibs
      )
      ++ lib.optionals stdenv.hostPlatform.isDarwin [
        expat
        fontconfig
        freetype
      ];

    nativeBuildInputs = [
      pkg-config
    ]
    ++ lib.optionals stdenv.hostPlatform.isLinux [ mold ]
    ++ lib.optionals (!stdenv.hostPlatform.isWindows) [ makeWrapper ];

    checkInputs = lib.optionals (!stdenv.hostPlatform.isWindows) [
      clang
      writableTmpDirAsHomeHook
    ];

    propagatedBuildInputs = lib.optionals (!stdenv.hostPlatform.isWindows) [
      android-tools
    ];

    postInstall = ''
      ${lib.optionalString stdenv.hostPlatform.isLinux ''
        wrapProgram $out/bin/uad-ng \
          --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath linuxRuntimeLibs} \
          --suffix PATH : ${lib.makeBinPath [ android-tools ]}
      ''}
      ${lib.optionalString stdenv.hostPlatform.isDarwin ''
        wrapProgram $out/bin/uad-ng \
          --suffix DYLD_FALLBACK_LIBRARY_PATH : ${lib.makeLibraryPath darwinRuntimeLibs} \
          --suffix PATH : ${lib.makeBinPath [ android-tools ]}
      ''}
    '';

    meta = {
      description = "Tool to debloat non-rooted Android devices";
      homepage = "https://github.com/Universal-Debloater-Alliance/universal-android-debloater-next-generation";
      license = lib.licenses.gpl3Only;
      mainProgram = "uad-ng";
      platforms = lib.platforms.linux ++ lib.platforms.darwin ++ lib.platforms.windows;
    };
  }
)
