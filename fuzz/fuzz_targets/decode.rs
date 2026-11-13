// SPDX-License-Identifier: AGPL-3.0-only

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = bomward_core::read_bytes(data, &bomward_core::Limits::default());
});
