//! Example runner for FiddlerScript programs.
//!
//! This example demonstrates how to use the FiddlerScript interpreter
//! to run scripts from strings or files.

use fiddler_script::Interpreter;

fn main() {
    println!("=== FiddlerScript Examples ===\n");

    // Example 1: Basic arithmetic
    println!("Example 1: Basic Arithmetic");
    run_example(
        r#"
let x = 10;
let y = 20;
let sum = x + y;
print("Sum:", sum);
"#,
    );

    // Example 2: Control flow
    println!("\nExample 2: Control Flow");
    run_example(
        r#"
let age = 18;
if (age >= 18) {
    print("You are an adult");
} else {
    print("You are a minor");
}
"#,
    );

    // Example 3: For loop
    println!("\nExample 3: For Loop");
    run_example(
        r#"
print("Counting from 1 to 5:");
for (let i = 1; i <= 5; i = i + 1) {
    print(i);
}
"#,
    );

    // Example 4: Functions
    println!("\nExample 4: Functions");
    run_example(
        r#"
fn add(a, b) {
    return a + b;
}

fn multiply(a, b) {
    return a * b;
}

let result = add(5, 3);
print("5 + 3 =", result);

let product = multiply(4, 7);
print("4 * 7 =", product);
"#,
    );

    // Example 5: Recursion (Factorial)
    println!("\nExample 5: Factorial");
    run_example(
        r#"
fn factorial(n) {
    if (n <= 1) {
        return 1;
    }
    return n * factorial(n - 1);
}

print("5! =", factorial(5));
print("10! =", factorial(10));
"#,
    );

    // Example 6: Fibonacci
    println!("\nExample 6: Fibonacci");
    run_example(
        r#"
fn fibonacci(n) {
    if (n <= 1) {
        return n;
    }
    return fibonacci(n - 1) + fibonacci(n - 2);
}

print("First 10 Fibonacci numbers:");
for (let i = 0; i < 10; i = i + 1) {
    print(fibonacci(i));
}
"#,
    );

    // Example 7: String operations
    println!("\nExample 7: String Operations");
    run_example(
        r#"
let greeting = "Hello";
let name = "World";
let message = greeting + " " + name + "!";
print(message);
print("Length:", len(message));
"#,
    );

    // Example 8: Boolean logic
    println!("\nExample 8: Boolean Logic");
    run_example(
        r#"
let a = true;
let b = false;

print("a =", a);
print("b =", b);
print("a && b =", a && b);
print("a || b =", a || b);
print("!a =", !a);
print("!b =", !b);
"#,
    );
}

fn run_example(source: &str) {
    let mut interpreter = Interpreter::new();
    match interpreter.run(source) {
        Ok(_) => {}
        Err(e) => eprintln!("Error: {}", e),
    }
}
