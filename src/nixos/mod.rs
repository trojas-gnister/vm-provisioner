//! NixOS configuration and image building
//!
//! This module replaces the Fedora kickstart pipeline with declarative
//! NixOS configuration generation and qcow2 image building via `nixos-generators`.

pub mod config_gen;
pub mod image_builder;
pub mod packages;
