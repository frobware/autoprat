{
  description = "autoprat - GitHub PR query and bulk-action helper";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.05";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, rust-overlay }:
  let
    supportedSystems = [
      "aarch64-darwin"
      "aarch64-linux"
      "x86_64-darwin"
      "x86_64-linux"
    ];

    forAllSystems = f: nixpkgs.lib.genAttrs supportedSystems (system: f {
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ rust-overlay.overlays.default ];
      };
      inherit system;
    });
  in
  {
    devShells = forAllSystems ({ pkgs, system }: {
      default = pkgs.mkShell {
        buildInputs = with pkgs; [
          # Nightly rustfmt for the unstable_features set in
          # rustfmt.toml (group_imports, imports_granularity). The
          # Makefile invokes `cargo +nightly fmt`, so this must be on
          # PATH ahead of the stable rustfmt.
          rust-bin.nightly.latest.rustfmt

          # Stable Rust pinned to rust-toolchain.toml's channel,
          # without rustfmt so it doesn't shadow the nightly one
          # above.
          (rust-bin.stable.latest.default.override {
            extensions = [ "rust-src" "rust-analyzer" "clippy" "llvm-tools-preview" ];
          })

          # Build inputs for openssl-sys (reqwest's native-tls path).
          pkg-config
          openssl

          # Workflow tools the Makefile drives.
          git
          gnumake

          # Coverage tool wired up by `make coverage*`.
          cargo-llvm-cov
        ];

        RUST_BACKTRACE = "1";
      };
    });
  };
}
