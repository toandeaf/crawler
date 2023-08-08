# Little Rusty Web Crawler

This is a brief overview of how to set up, build, and run this web crawler project.

## Prerequisites

- A modern operating system. 
- The rust compile and cargo toolchains must be installed.

## Installing Rust

1. **Install `rustup`**, the Rust toolchain installer. Visit [rustup.rs](https://rustup.rs/) and follow the instructions.

2. After installing `rustup`, open a new terminal or command prompt and **verify the installation**:

```bash
rustc --version
cargo --version
```

## Running the Project

1. Navigate to the project directory, the `Cargo.toml` should be local.

2. To build the project and run, enter the following commands.

```bash
cargo build
cargo run "https://example.com"
```

### Notes
* Command line argument required to run program - Also must be a valid URL in the format "https://{domain}.{id}"
* As part of this implementation, two files will be created to identify the links by page, and the total unique links across the site.
