{
  description = "Deciduous - Decision graph tooling for AI-assisted development";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    crane = {
      url = "github:ipetkov/crane";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, crane }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        # Rust toolchain - use stable with minimum version from Cargo.toml (1.70)
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" "clippy" ];
        };

        # Initialize crane with our Rust toolchain
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        # Common source filtering for Rust builds
        src = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter = path: type:
            # Include Cargo files
            (pkgs.lib.hasSuffix "Cargo.toml" path) ||
            (pkgs.lib.hasSuffix "Cargo.lock" path) ||
            # Include Rust source
            (pkgs.lib.hasInfix "/src/" path) ||
            (pkgs.lib.hasInfix "/bin/" path) ||
            (pkgs.lib.hasSuffix ".rs" path) ||
            # Include migrations for diesel
            (pkgs.lib.hasInfix "/migrations/" path) ||
            # Include the viewer HTML (embedded in binary)
            (pkgs.lib.hasSuffix "viewer.html" path) ||
            # Default crane filter (for build.rs, etc.)
            (craneLib.filterCargoSources path type);
        };

        # Platform-specific dependencies for macOS
        # libiconv is needed for diesel/sqlite bindings
        darwinDeps = pkgs.lib.optionals pkgs.stdenv.isDarwin [
          pkgs.libiconv
        ];

        # Common build inputs for Rust
        commonBuildInputs = [
          pkgs.sqlite
          pkgs.openssl
        ] ++ darwinDeps;

        # Common native build inputs
        commonNativeBuildInputs = [
          pkgs.pkg-config
        ];

        # Common environment variables for builds
        commonEnv = {
          # Use bundled SQLite (matches Cargo.toml libsqlite3-sys bundled feature)
          SQLITE3_STATIC = "1";
        } // pkgs.lib.optionalAttrs pkgs.stdenv.isDarwin {
          # macOS needs libiconv in library path
          LIBRARY_PATH = "${pkgs.libiconv}/lib";
        };

        # Build trace-interceptor (Node.js)
        traceInterceptor = pkgs.buildNpmPackage {
          pname = "deciduous-trace-interceptor";
          version = "0.1.0";
          src = ./trace-interceptor;
          npmDepsHash = "sha256-Vq918124VdB1h+NzqD1bTiNe2k7c+xcjg01KIlU0cdM=";

          # Skip default npm build
          dontNpmBuild = true;

          buildPhase = ''
            runHook preBuild
            npm run build
            npm run bundle
            runHook postBuild
          '';

          installPhase = ''
            runHook preInstall
            mkdir -p $out/dist
            cp -r dist/* $out/dist/
            runHook postInstall
          '';
        };

        # Build web viewer (Node.js + Vite)
        webViewer = pkgs.buildNpmPackage {
          pname = "deciduous-viewer";
          version = "0.1.0";
          src = ./web;
          npmDepsHash = "sha256-2+OTxPgKsLI8uH3NOO3ebWi6QsIRvfnivWZa24DRPcQ=";

          buildPhase = ''
            runHook preBuild
            npm run build
            runHook postBuild
          '';

          installPhase = ''
            runHook preInstall
            mkdir -p $out/dist
            cp -r dist/* $out/dist/
            runHook postInstall
          '';
        };

        # Read Cargo.toml contents for crane (needed when source is a derivation)
        cargoTomlContents = builtins.readFile ./Cargo.toml;

        # Minimal source with trace-interceptor (no web viewer)
        # The Rust code requires trace-interceptor/dist/bundle.js at compile time
        srcMinimal = pkgs.runCommand "deciduous-src-minimal" { } ''
          cp -r ${src} $out
          chmod -R u+w $out
          mkdir -p $out/trace-interceptor/dist
          cp ${traceInterceptor}/dist/bundle.js $out/trace-interceptor/dist/bundle.js
        '';

        # Cargo artifacts (dependencies only) - speeds up rebuilds
        cargoArtifacts = craneLib.buildDepsOnly ({
          pname = "deciduous";
          version = "0.8.15";
          src = srcMinimal;
          inherit cargoTomlContents;
          buildInputs = commonBuildInputs;
          nativeBuildInputs = commonNativeBuildInputs;
        } // commonEnv);

        # Main deciduous binary (minimal - no embedded web viewer)
        deciduous = craneLib.buildPackage ({
          pname = "deciduous";
          version = "0.8.15";
          src = srcMinimal;
          inherit cargoTomlContents cargoArtifacts;
          buildInputs = commonBuildInputs;
          nativeBuildInputs = commonNativeBuildInputs;

          meta = with pkgs.lib; {
            description = "Decision graph tooling for AI-assisted development";
            homepage = "https://github.com/notactuallytreyanastasio/deciduous";
            license = licenses.mit;
            maintainers = [ ];
            mainProgram = "deciduous";
          };
        } // commonEnv);

        # Full release source with embedded web viewer and trace interceptor
        # Uses the filtered source as base, patches in the built artifacts
        srcFull = pkgs.runCommand "deciduous-src-full" { } ''
          cp -r ${src} $out
          chmod -R u+w $out
          cp ${webViewer}/dist/index.html $out/src/viewer.html
          mkdir -p $out/trace-interceptor/dist
          cp ${traceInterceptor}/dist/bundle.js $out/trace-interceptor/dist/bundle.js
        '';

        # Cargo artifacts for full build
        # Must provide cargoTomlContents since srcFull is a derivation
        cargoArtifactsFull = craneLib.buildDepsOnly ({
          pname = "deciduous";
          version = "0.8.15";
          src = srcFull;
          inherit cargoTomlContents;
          buildInputs = commonBuildInputs;
          nativeBuildInputs = commonNativeBuildInputs;
        } // commonEnv);

        # Full release binary with embedded web viewer (equivalent to make release-full)
        deciduousFull = craneLib.buildPackage ({
          pname = "deciduous";
          version = "0.8.15";
          src = srcFull;
          inherit cargoTomlContents;
          cargoArtifacts = cargoArtifactsFull;
          buildInputs = commonBuildInputs;
          nativeBuildInputs = commonNativeBuildInputs;

          meta = with pkgs.lib; {
            description = "Decision graph tooling for AI-assisted development (with embedded web viewer)";
            homepage = "https://github.com/notactuallytreyanastasio/deciduous";
            license = licenses.mit;
            maintainers = [ ];
            mainProgram = "deciduous";
          };
        } // commonEnv);

        # Helper script: menu
        menu = pkgs.writeShellScriptBin "menu" ''
          echo "========================================"
          echo "  Deciduous Development Shell (Nix)"
          echo "========================================"
          echo ""
          echo "Cargo commands:"
          echo "  cargo build --release    Build release binary (uses existing viewer.html)"
          echo "  cargo test               Run tests"
          echo "  cargo clippy             Run linter"
          echo "  cargo fmt                Format code"
          echo ""
          echo "Nix build commands:"
          echo "  nix build                Full build with embedded web viewer (default)"
          echo "  nix build .#minimal      Minimal build without rebuilding web viewer"
          echo "  nix build .#webViewer    Build web viewer only"
          echo "  nix build .#traceInterceptor  Build trace interceptor only"
          echo ""
          echo "Nix run/check commands:"
          echo "  nix run                  Run deciduous (full build)"
          echo "  nix flake check          Run all checks (build, clippy, test, fmt)"
          echo ""
          echo "Dev workflow (impure, modifies source tree):"
          echo "  nix-build-full           Build everything locally (like make release-full)"
          echo ""
          echo "Help:"
          echo "  menu                     Show this menu"
          echo ""
        '';

        # Helper script: nix-build-full (equivalent to make release-full)
        nixBuildFull = pkgs.writeShellScriptBin "nix-build-full" ''
          set -e
          echo "Building trace-interceptor..."
          (cd trace-interceptor && npm install && npm run build && npm run bundle)

          echo "Building web viewer..."
          (cd web && npm install && npm run build)

          echo "Copying viewer to src/..."
          cp web/dist/index.html src/viewer.html
          cp web/dist/index.html docs/demo/index.html

          echo "Clearing trace interceptor cache..."
          rm -rf ~/.deciduous/trace-interceptor

          echo "Building Rust binary..."
          cargo build --release

          echo ""
          echo "Full release build complete: target/release/deciduous"
        '';

      in
      {
        # Packages
        packages = {
          # Default is the full build with embedded web viewer
          default = deciduousFull;
          full = deciduousFull;
          # Minimal build without web viewer (faster, smaller)
          minimal = deciduous;
          inherit deciduous deciduousFull traceInterceptor webViewer;
        };

        # Checks (run with `nix flake check`)
        # Note: checks use srcMinimal which includes trace-interceptor bundle
        checks = {
          # Build the package
          inherit deciduous;

          # Run clippy
          deciduous-clippy = craneLib.cargoClippy ({
            pname = "deciduous";
            version = "0.8.15";
            src = srcMinimal;
            inherit cargoTomlContents cargoArtifacts;
            buildInputs = commonBuildInputs;
            nativeBuildInputs = commonNativeBuildInputs;
            cargoClippyExtraArgs = "--all-targets -- -D warnings";
          } // commonEnv);

          # Run tests
          deciduous-test = craneLib.cargoTest ({
            pname = "deciduous";
            version = "0.8.15";
            src = srcMinimal;
            inherit cargoTomlContents cargoArtifacts;
            buildInputs = commonBuildInputs;
            nativeBuildInputs = commonNativeBuildInputs;
          } // commonEnv);

          # Check formatting (uses original src since formatting doesn't need trace-interceptor)
          deciduous-fmt = craneLib.cargoFmt {
            pname = "deciduous";
            version = "0.8.15";
            src = srcMinimal;
            inherit cargoTomlContents;
          };
        };

        # Apps (run with `nix run`)
        apps = {
          default = flake-utils.lib.mkApp {
            drv = deciduousFull;
          };
          deciduous = flake-utils.lib.mkApp {
            drv = deciduousFull;
          };
          minimal = flake-utils.lib.mkApp {
            drv = deciduous;
          };
        };

        # Development shell
        devShells.default = craneLib.devShell {
          # Include checks to get build inputs
          checks = self.checks.${system};

          # Additional packages for development
          packages = [
            # DevShell helper scripts
            menu
            nixBuildFull

            # Node.js for web viewer and trace-interceptor
            pkgs.nodejs_20
            pkgs.nodePackages.npm
            pkgs.nodePackages.typescript

            # SQLite tools
            pkgs.sqlite

            # Optional: graphviz for DOT -> PNG conversion
            pkgs.graphviz

            # Optional: diesel CLI for database migrations
            pkgs.diesel-cli

            # Git (usually already available, but explicit)
            pkgs.git

            # Useful development tools
            pkgs.cargo-watch
            pkgs.cargo-edit
          ] ++ darwinDeps;

          # Environment variables for development
          shellHook = ''
            # Ensure Rust tools are available
            export RUST_SRC_PATH="${rustToolchain}/lib/rustlib/src/rust/library"

            ${pkgs.lib.optionalString pkgs.stdenv.isDarwin ''
              # macOS: Set library path for libiconv (needed by diesel/sqlite)
              export LIBRARY_PATH="${pkgs.libiconv}/lib:''${LIBRARY_PATH:-}"
            ''}

            # Show menu on shell entry
            menu
          '';

          # Set environment variables
          SQLITE3_STATIC = "1";
        };

        # Formatter for `nix fmt`
        formatter = pkgs.nixpkgs-fmt;
      }
    );
}
