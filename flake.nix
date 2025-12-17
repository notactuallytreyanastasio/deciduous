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
          npmDepsHash = "sha256-31qclPLawpYm05FW7M0voHKLxwXSneImZjhGYkOx3c4=";

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

        # Cargo artifacts (dependencies only) - speeds up rebuilds
        cargoArtifacts = craneLib.buildDepsOnly ({
          inherit src;
          buildInputs = commonBuildInputs;
          nativeBuildInputs = commonNativeBuildInputs;
        } // commonEnv);

        # Main deciduous binary
        deciduous = craneLib.buildPackage ({
          inherit src cargoArtifacts;
          buildInputs = commonBuildInputs;
          nativeBuildInputs = commonNativeBuildInputs;

          # Run clippy as part of the build for extra safety
          cargoClippyExtraArgs = "--all-targets -- -D warnings";

          meta = with pkgs.lib; {
            description = "Decision graph tooling for AI-assisted development";
            homepage = "https://github.com/notactuallytreyanastasio/deciduous";
            license = licenses.mit;
            maintainers = [ ];
            mainProgram = "deciduous";
          };
        } // commonEnv);

      in
      {
        # Packages
        packages = {
          default = deciduous;
          inherit deciduous traceInterceptor webViewer;
        };

        # Checks (run with `nix flake check`)
        checks = {
          # Build the package
          inherit deciduous;

          # Run clippy
          deciduous-clippy = craneLib.cargoClippy ({
            inherit src cargoArtifacts;
            buildInputs = commonBuildInputs;
            nativeBuildInputs = commonNativeBuildInputs;
            cargoClippyExtraArgs = "--all-targets -- -D warnings";
          } // commonEnv);

          # Run tests
          deciduous-test = craneLib.cargoTest ({
            inherit src cargoArtifacts;
            buildInputs = commonBuildInputs;
            nativeBuildInputs = commonNativeBuildInputs;
          } // commonEnv);

          # Check formatting
          deciduous-fmt = craneLib.cargoFmt {
            inherit src;
          };
        };

        # Apps (run with `nix run`)
        apps = {
          default = flake-utils.lib.mkApp {
            drv = deciduous;
          };
          deciduous = flake-utils.lib.mkApp {
            drv = deciduous;
          };
        };

        # Development shell
        devShells.default = craneLib.devShell {
          # Include checks to get build inputs
          checks = self.checks.${system};

          # Additional packages for development
          packages = with pkgs; [
            # Rust tools (from crane's devShell via rustToolchain)
            # rustToolchain is already included by craneLib.devShell

            # Node.js for web viewer and trace-interceptor
            nodejs_20
            nodePackages.npm
            nodePackages.typescript

            # SQLite tools
            sqlite

            # Optional: graphviz for DOT -> PNG conversion
            graphviz

            # Optional: diesel CLI for database migrations
            diesel-cli

            # Git (usually already available, but explicit)
            git

            # Useful development tools
            cargo-watch
            cargo-edit
          ] ++ darwinDeps;

          # Environment variables for development
          shellHook = ''
            # Ensure Rust tools are available
            export RUST_SRC_PATH="${rustToolchain}/lib/rustlib/src/rust/library"

            ${pkgs.lib.optionalString pkgs.stdenv.isDarwin ''
              # macOS: Set library path for libiconv (needed by diesel/sqlite)
              export LIBRARY_PATH="${pkgs.libiconv}/lib:''${LIBRARY_PATH:-}"
            ''}

            # Helper function for full builds (equivalent to make release-full)
            nix-build-full() {
              echo "Building trace-interceptor..."
              (cd trace-interceptor && npm install && npm run build && npm run bundle) || return 1

              echo "Building web viewer..."
              (cd web && npm install && npm run build) || return 1

              echo "Copying viewer to src/..."
              cp web/dist/index.html src/viewer.html
              cp web/dist/index.html docs/demo/index.html

              echo "Clearing trace interceptor cache..."
              rm -rf ~/.deciduous/trace-interceptor

              echo "Building Rust binary..."
              cargo build --release || return 1

              echo ""
              echo "Full release build complete: target/release/deciduous"
            }

            # Export the function
            export -f nix-build-full

            echo "========================================"
            echo "  Deciduous Development Shell (Nix)"
            echo "========================================"
            echo ""
            echo "Rust commands:"
            echo "  cargo build --release    Build release binary"
            echo "  cargo test               Run tests"
            echo "  cargo clippy             Run linter"
            echo "  cargo fmt                Format code"
            echo ""
            echo "Nix commands:"
            echo "  nix build                Build deciduous package"
            echo "  nix build .#webViewer    Build web viewer only"
            echo "  nix build .#traceInterceptor  Build trace interceptor"
            echo "  nix flake check          Run all checks (build, clippy, test, fmt)"
            echo ""
            echo "Full rebuild (equivalent to make release-full):"
            echo "  nix-build-full"
            echo ""
          '';

          # Set environment variables
          SQLITE3_STATIC = "1";
        };

        # Formatter for `nix fmt`
        formatter = pkgs.nixpkgs-fmt;
      }
    );
}
