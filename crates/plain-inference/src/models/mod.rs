pub mod default_model;

pub mod manifest;

pub(crate) mod package;

#[cfg(any(test, feature = "test-seams"))]
#[doc(hidden)]
pub mod test_fixture;
