{
  description = "darask-IME: Japanese IME with smarter candidates (fcitx5)";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };

      # Build everything against an arbitrary pkgs, so the addon can be compiled against the
      # same fcitx5 your system runs (see overlays.default).
      mk = pkgs:
        let
          lib = pkgs.lib;
          src = lib.cleanSourceWith {
            src = ./.;
            filter = path: type:
              let b = baseNameOf path; in !(builtins.elem b [ "target" "result" ".git" ]);
          };
          cargoLock = { lockFile = ./Cargo.lock; };

          cmudict = pkgs.fetchurl {
            url = "https://raw.githubusercontent.com/cmusphinx/cmudict/master/cmudict.dict";
            hash = "sha256-gZF4Q8f0TOKwlKxjhzwsekz4AgQHksRVujykBokcPSI=";
          };
          wordfreq = pkgs.fetchurl {
            url = "https://norvig.com/ngrams/count_1w.txt";
            hash = "sha256-Ud8Vn9PeErIOQDwQj1JultvXI9nKvdXxeVXNwWBZ5pA=";
          };

          tools = pkgs.rustPlatform.buildRustPackage {
            pname = "darask-tools";
            version = "0.1.0";
            inherit src cargoLock;
            cargoBuildFlags = [ "-p" "darask-build" "-p" "darask-cli" ];
            doCheck = false;
          };

          data = pkgs.runCommand "darask-data" { } ''
            mkdir -p src $out/share/darask-ime
            cp ${cmudict} src/cmudict.dict
            cp ${wordfreq} src/count_1w.txt
            cp ${./data/src/extra_en.tsv} src/extra_en.tsv
            ${tools}/bin/darask-build ${pkgs.mozc.src} src $out/share/darask-ime
          '';

          addon = pkgs.stdenv.mkDerivation {
            pname = "fcitx5-darask";
            version = "0.1.0";
            inherit src;
            cargoDeps = pkgs.rustPlatform.importCargoLock cargoLock;
            nativeBuildInputs = with pkgs; [ cmake kdePackages.extra-cmake-modules pkg-config cargo rustc rustPlatform.cargoSetupHook ];
            buildInputs = [ pkgs.fcitx5 pkgs.fmt ];
            dontUseCmakeConfigure = true;
            buildPhase = ''
              runHook preBuild
              cargo build --release -p darask-ffi --offline
              cmake -S addon -B addon-build \
                -DCMAKE_BUILD_TYPE=Release \
                -DCMAKE_INSTALL_PREFIX=$out \
                -DDARASK_FFI_LIB=$PWD/target/release/libdarask_ffi.a \
                -DDARASK_DATA_DIR=${data}/share/darask-ime
              cmake --build addon-build -j$NIX_BUILD_CORES
              runHook postBuild
            '';
            installPhase = ''
              runHook preInstall
              cmake --install addon-build
              runHook postInstall
            '';
          };

          e2e = pkgs.runCommand "darask-e2e" { } ''
            export HOME=$TMPDIR XDG_RUNTIME_DIR=$TMPDIR
            export XDG_DATA_HOME=$TMPDIR/data XDG_CONFIG_HOME=$TMPDIR/cfg
            mkdir -p share/fcitx5/addon
            cp ${pkgs.fcitx5}/share/fcitx5/testing/addon/*.conf share/fcitx5/addon/
            export XDG_DATA_DIRS=$PWD/share:${addon}/share:${pkgs.fcitx5}/share
            export FCITX_ADDON_DIRS=${addon}/lib/fcitx5:${pkgs.fcitx5}/lib/fcitx5
            export DARASK_DATA=${data}/share/darask-ime
            ${addon}/libexec/darask-e2e
            touch $out
          '';
        in { inherit tools data addon e2e; };

      built = mk pkgs;
    in {
      packages.${system} = { inherit (built) tools data addon; default = built.addon; };
      checks.${system}.e2e = built.e2e;

      # nixpkgs.overlays = [ darask-ime.overlays.default ];  → pkgs.fcitx5-darask
      overlays.default = final: prev: { fcitx5-darask = (mk final).addon; fcitx5-darask-e2e = (mk final).e2e; };

      devShells.${system}.default = pkgs.mkShell {
        packages = with pkgs; [ cargo rustc rust-analyzer clippy rustfmt pkg-config cmake kdePackages.extra-cmake-modules gcc fcitx5 fmt curl gzip ];
        MOZC_SRC = "${pkgs.mozc.src}";
      };
    };
}
