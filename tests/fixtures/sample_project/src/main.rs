//! Sample Rust file for testing
//!
//! <!--Q:begin id=sample.main tags=code,rust,main v=1-->
//! This is the main entry point for the sample project.
//! <!--Q:end id=sample.main-->

fn main() {
    println!("Hello from sample project!");
}

// <!--Q:begin id=sample.helper tags=code,rust,helper v=1-->
fn helper_function() -> i32 {
    42
}
// <!--Q:end id=sample.helper-->

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_helper() {
        assert_eq!(helper_function(), 42);
    }
}
