// Copyright (c) Sean Lawlor
//
// This source code is licensed under both the MIT license found in the
// LICENSE-MIT file in the root directory of this source tree.

//! Time/timer related functionality built on [ractor]

#![allow(clippy::duplicate_mod)]

#[path = "cron/common.rs"]
mod cron_common;

#[cfg(feature = "time")]
#[deprecated = "Use \"cron-0-12\" feature (or a newer one) rather than \"time\""]
pub mod cron {
    use cron_0_12 as cron_crate;
    #[path = "mod.rs"]
    mod cron_impl;
    pub use self::cron_impl::*;
    pub use super::cron_common::*;
}

#[cfg(feature = "cron-0-12")]
#[path = "cron"]
pub mod cron_0_12 {
    use cron_0_12 as cron_crate;
    #[path = "mod.rs"]
    mod cron_impl;
    pub use self::cron_impl::*;
    pub use super::cron_common::*;
}

#[cfg(feature = "cron-0-13")]
#[path = "cron"]
pub mod cron_0_13 {
    use cron_0_13 as cron_crate;
    #[path = "mod.rs"]
    mod cron_impl;
    pub use self::cron_impl::*;
    pub use super::cron_common::*;
}

#[cfg(feature = "cron-0-14")]
#[path = "cron"]
pub mod cron_0_14 {
    use cron_0_14 as cron_crate;
    #[path = "mod.rs"]
    mod cron_impl;
    pub use self::cron_impl::*;
    pub use super::cron_common::*;
}

#[cfg(feature = "cron-0-15")]
#[path = "cron"]
pub mod cron_0_15 {
    use cron_0_15 as cron_crate;
    #[path = "mod.rs"]
    mod cron_impl;
    pub use self::cron_impl::*;
    pub use super::cron_common::*;
}

#[cfg(feature = "cron-0-16")]
#[path = "cron"]
pub mod cron_0_16 {
    use cron_0_16 as cron_crate;
    #[path = "mod.rs"]
    mod cron_impl;
    pub use self::cron_impl::*;
    pub use super::cron_common::*;
}
