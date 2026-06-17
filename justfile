# Manch task runner. Run `just` to list recipes.

# List available recipes
default:
    @just --list

# Remove Cargo build artifacts not accessed in the last day (needs `cargo install cargo-sweep`)
sweep:
    cargo sweep --time 1
