{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
    pre-commit-hooks.url = "github:cachix/git-hooks.nix";
    workflow-parts.url = "github:valeratrades/.github?dir=.github/workflows/nix-parts";
  };

  outputs = { nixpkgs, rust-overlay, flake-utils, pre-commit-hooks, workflow-parts, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = builtins.trace "flake.nix sourced" [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        checks = {
          pre-commit-check = pre-commit-hooks.lib.${system}.run {
            src = ./.;
            hooks = {
              nixpkgs-fmt.enable = true;
              #rustfmt.enable = true;


              ##TODO!!!: configure using https://github.com/cachix/git-hooks.nix?tab=readme-ov-file#custom-hooks
              # In reality right now it's just copying over thinks from file_snippet presets
              test = {
                enable = true;
                #entry = ''notify-send "Test hook goes brrrr" -t 999999'';
                #files = "Cargo.toml";
                entry =
                  let
                    shared_flags = ''--grouped --order package,lints,dependencies,dev-dependencies,build-dependencies,features'';
                  in
                  #''cargo sort --workspace ${shared_flags} || cargo sort ${shared_flags}'';
                    #''cargo sort''; # --grouped'';
                    #''notify-send "(pwd)" -t 999999; cargo sort -c .'';
                    #''
                    #  cargo sort . ${shared_flags}''# || echo "It likely sorted the files successfully, and now struggling with `crate folder not found` for whatever reason it does that"''

                    #''notify-send -t 999999'';
                  ''cargo sort'';

                files = "Cargo.toml";
                stages = [ "pre-commit" ];
                language = "rust";
              };
            };
          };
        };
        workflowContents = (import ./.github/workflows/ci.nix) { inherit pkgs workflow-parts; };
      in
      {
        packages =
          let
            manifest = (pkgs.lib.importTOML ./Cargo.toml).package;
            rust = (pkgs.rust-bin.fromRustupToolchainFile ./.cargo/rust-toolchain.toml);
            rustc = rust;
            cargo = rust;
            stdenv = pkgs.stdenvAdapters.useMoldLinker pkgs.stdenv;
            rustPlatform = pkgs.makeRustPlatform {
              inherit rustc cargo stdenv;
            };
          in
          {
            default = rustPlatform.buildRustPackage rec {
              pname = manifest.name;
              version = manifest.version;

              buildInputs = with pkgs; [
                openssl
                openssl.dev
              ];
              nativeBuildInputs = with pkgs; [ pkg-config ];
              env.PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
              #stdenv = pkgs.stdenvAdapters.useMoldLinker pkgs.stdenv;

              cargoLock.lockFile = ./Cargo.lock;
              src = pkgs.lib.cleanSource ./.;
            };
          };

        devShells.default = with pkgs; mkShell {
          inherit stdenv;
          shellHook = checks.pre-commit-check.shellHook + ''
            rm -f ./.github/workflows/errors.yml; cp ${workflowContents.errors} ./.github/workflows/errors.yml
            rm -f ./.github/workflows/warnings.yml; cp ${workflowContents.warnings} ./.github/workflows/warnings.yml

            cargo -Zscript -q ./tmp/script.rs ./.git/hooks/pre-commit
          '';
          packages = [
            mold-wrapped
            openssl
            pkg-config
            (rust-bin.fromRustupToolchainFile ./.cargo/rust-toolchain.toml)
          ] ++ checks.pre-commit-check.enabledPackages;
        };
      }
    );
}

