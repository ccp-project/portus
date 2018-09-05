//! This crate implements integration tests for libccp and portus.
//! Each integration test consists of the following;:
//! 1. A struct IntegrationTest<T: Ipc>, along with the Impl.
//!     This could be a tuple struct around TestBase, declared as:
//!     pub struct IntegrationTest<T: Ipc>(TestBase<T>)
//! 2. Any additional structs for use with struct IntegrationTest, 
//!     such as an IntegrationTestMeasurements struct
//! 3. impl<T: Ipc> CongAlg<T> for IntegrationTest<T>
//!     - This contains the onCreate() and onReport().
//!     on_create() might install a program implemented in the IntegrationTest,
//!     while on_report() might contain a checker function for the test.
//!     on_report MUST send "Done" on the channel to end the test properly.

extern crate clap;
extern crate time;
#[macro_use]
extern crate slog;
extern crate portus;

pub mod scenarios;
