// Sample Rust file for mise tests

fn main() {
    println!("hello alpha");

    // TODO: remove debug output
    let x = 1;
    if x == 1 {
        println!("branch");
    }

    // Intentionally include an unsafe block for AST search demos.
    unsafe {
        let p: *const i32 = &x;
        let _y = *p;
    }
}
