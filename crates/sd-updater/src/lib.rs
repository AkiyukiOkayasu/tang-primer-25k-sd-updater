//! Gowin + PicoRV32 前提の SD カード更新機能のコア実装。
//!
//! `BoardIo` を実装して `Updater::new(io, spec)` に渡すだけで、
//! SD 初期化 → FAT ファイル探索 → header 検証 → payload 検証 → Flash 書き込み → 再構成 の
//! 一連の更新フローが動く。プロジェクト固有の値 (update file 名、magic、target ID、flash layout) は
//! [`UpdateSpec`] で受け取る。

#![cfg_attr(not(test), no_std)]

pub mod board;
pub mod crc32;
pub mod package;
pub mod sd_spi;
pub mod sha256;
pub mod updater;
pub mod w25q64;

mod spec;

pub use board::{BoardIo, IoError};
pub use spec::{MAX_HEADER_SIZE, UpdateSpec};
pub use updater::{UpdateStatus, Updater};

#[cfg(test)]
mod test_doubles;
