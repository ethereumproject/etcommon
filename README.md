# Etcommon

[![Build Status](https://travis-ci.org/ETCDEVTeam/etcommon-rs.svg?branch=master)](https://travis-ci.org/ETCDEVTeam/etcommon-rs)

Apache-2 licensed common Ethereum structs shared by crates. Work-in-progress right now.

## List of Crates

Below are all crates provided by the etcommon project.

| Name | Description | Crates.io | Documentation |
|------|:-----------:|:---------:|:-------------:|
| etcommon-rlp | Recursive-length prefix encoding, decoding, and compression | [![crates.io](https://img.shields.io/crates/v/etcommon-rlp.svg)](https://crates.io/crates/etcommon-rlp) | [![Documentation](https://docs.rs/etcommon-rlp/badge.svg)](https://docs.rs/etcommon-rlp) |
| etcommon-bigint | Big integer and hash implementation | [![crates.io](https://img.shields.io/crates/v/etcommon-bigint.svg)](https://crates.io/crates/etcommon-bigint) | [![Documentation](https://docs.rs/etcommon-bigint/badge.svg)](https://docs.rs/etcommon-bigint) |
| etcommon-hexutil | Small hex decoding helpers | [![crates.io](https://img.shields.io/crates/v/etcommon-hexutil.svg)](https://crates.io/crates/etcommon-hexutil) | [![Documentation](https://docs.rs/etcommon-hexutil/badge.svg)](https://docs.rs/etcommon-hexutil) |
| etcommon-bloom | Log bloom for Ethereum | [![crates.io](https://img.shields.io/crates/v/etcommon-bloom.svg)](https://crates.io/crates/etcommon-bloom) | [![Documentation](https://docs.rs/etcommon-bloom/badge.svg)](https://docs.rs/etcommon-bloom) |
| etcommon-trie | Merkle Trie specialized for Ethereum | [![crates.io](https://img.shields.io/crates/v/etcommon-trie.svg)](https://crates.io/crates/etcommon-trie) | [![Documentation](https://docs.rs/etcommon-trie/badge.svg)](https://docs.rs/etcommon-trie) |
| etcommon-block | Block, transaction and account structs for Ethereum | [![crates.io](https://img.shields.io/crates/v/etcommon-block.svg)](https://crates.io/crates/etcommon-block) | [![Documentation](https://docs.rs/etcommon-block/badge.svg)](https://docs.rs/etcommon-block) |
| etcommon-block-core | Core block, transaction and account structs for Ethereum | [![crates.io](https://img.shields.io/crates/v/etcommon-block-core.svg)](https://crates.io/crates/etcommon-block-core) | [![Documentation](https://docs.rs/etcommon-block-core/badge.svg)](https://docs.rs/etcommon-block-core) |

## Rolling Release

As `etcommon` will be depended on many other projects like SputnikVM and EASM, rolling release is necessary to keep up with the development speed.

Note that rolling release will only happen for sub-crates like `etcommon-bigint`, `etcommon-rlp`, `etcommon-util`, etc. The top-level crate `etcommon` will follow the normal release process.

## Usage
The repositories on crates.io are labeled with the prefix `etcommon-`, however the crates themselves do not have this prefix so you'll need to specify them as `etcommon-PACKAGE = "^0"` in `Cargo.toml` and as `extern crate PACKAGE;` in your `main.rs`

### etcommon-rlp
Cargo.toml: `etcommon-rlp = "^0"`

main.rs: `extern crate rlp;`

### etcommon-bigint
Cargo.toml: `etcommon-bigint = "^0"`

main.rs: `extern crate bigint;`

### etcommon-hexutil
Cargo.toml: `etcommon-hexutil = "^0"`

main.rs: `extern crate hexutil;`

### etcommon-trie
Cargo.toml: `etcommon-trie = "^0"`

main.rs: `extern crate trie;`

### etcommon-block
Cargo.toml: `etcommon-block = "^0"`

main.rs: `extern crate block;`

### etcommon-block-core
Cargo.toml: `etcommon-block-core = "^0"`

main.rs: `extern crate block_core;`
