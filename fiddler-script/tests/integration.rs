//! Integration tests for FiddlerScript.
//!
//! These tests verify complete programs run correctly and match the
//! expected behavior documented in INTERPRETER.md.

use fiddler_script::{Interpreter, Value};

fn run(source: &str) -> Value {
    let mut interpreter = Interpreter::new();
    interpreter.run(source).expect("Script execution failed")
}

// === Test Cases from INTERPRETER.md ===

#[test]
fn test_basic_arithmetic() {
    // Test 1: Basic Arithmetic
    let source = r#"
        let x = 5;
        let y = 3;
        x + y;
    "#;
    assert_eq!(run(source), Value::Integer(8));

    let source = r#"
        let x = 5;
        let y = 3;
        x - y;
    "#;
    assert_eq!(run(source), Value::Integer(2));

    let source = r#"
        let x = 5;
        let y = 3;
        x * y;
    "#;
    assert_eq!(run(source), Value::Integer(15));

    let source = r#"
        let x = 5;
        let y = 3;
        x / y;
    "#;
    assert_eq!(run(source), Value::Integer(1));
}

#[test]
fn test_if_else() {
    // Test 2: If-Else
    let source = r#"
        let x = 10;
        let result = "";
        if (x > 5) {
            result = "Greater";
        } else {
            result = "Smaller";
        }
        result;
    "#;
    assert_eq!(run(source), Value::String("Greater".to_string()));
}

#[test]
fn test_for_loop() {
    // Test 3: For Loop
    let source = r#"
        let sum = 0;
        for (let i = 0; i < 3; i = i + 1) {
            sum = sum + i;
        }
        sum;
    "#;
    assert_eq!(run(source), Value::Integer(3)); // 0 + 1 + 2
}

#[test]
fn test_nested_if() {
    // Test 4: Nested If
    let source = r#"
        let x = 15;
        let result = "";
        if (x > 10) {
            if (x > 20) {
                result = "very large";
            } else {
                result = "medium";
            }
        } else {
            result = "small";
        }
        result;
    "#;
    assert_eq!(run(source), Value::String("medium".to_string()));
}

#[test]
fn test_function_with_recursion() {
    // Test 5: Function with Recursion (Factorial)
    let source = r#"
        fn factorial(n) {
            if (n <= 1) {
                return 1;
            }
            return n * factorial(n - 1);
        }
        factorial(5);
    "#;
    assert_eq!(run(source), Value::Integer(120));
}

#[test]
fn test_string_concatenation() {
    // Test 6: String Concatenation
    let source = r#"
        let greeting = "Hello";
        let name = "World";
        greeting + " " + name;
    "#;
    assert_eq!(run(source), Value::String("Hello World".to_string()));
}

#[test]
fn test_boolean_expressions() {
    // Test 7: Boolean Expressions
    let source = r#"
        let a = true;
        let b = false;
        let result1 = a && !b;
        let result2 = a || b;
        result1 && result2;
    "#;
    assert_eq!(run(source), Value::Boolean(true));
}

// === Example Programs from INTERPRETER.md ===

#[test]
fn test_example_variables_and_arithmetic() {
    let source = r#"
        // Basic arithmetic
        let x = 10;
        let y = 20;
        let sum = x + y;
        sum;
    "#;
    assert_eq!(run(source), Value::Integer(30));
}

#[test]
fn test_example_control_flow() {
    let source = r#"
        // If-else statement
        let age = 18;
        let result = "";
        if (age >= 18) {
            result = "Adult";
        } else {
            result = "Minor";
        }
        result;
    "#;
    assert_eq!(run(source), Value::String("Adult".to_string()));
}

#[test]
fn test_example_for_loop() {
    let source = r#"
        // Count from 1 to 5 and sum them
        let sum = 0;
        for (let i = 1; i <= 5; i = i + 1) {
            sum = sum + i;
        }
        sum;
    "#;
    assert_eq!(run(source), Value::Integer(15)); // 1+2+3+4+5
}

#[test]
fn test_example_functions() {
    let source = r#"
        // Function definition and call
        fn add(a, b) {
            return a + b;
        }

        let result = add(5, 3);
        result;
    "#;
    assert_eq!(run(source), Value::Integer(8));
}

#[test]
fn test_example_fibonacci() {
    let source = r#"
        fn fibonacci(n) {
            if (n <= 1) {
                return n;
            }
            return fibonacci(n - 1) + fibonacci(n - 2);
        }

        // Test first few fibonacci numbers
        let f0 = fibonacci(0);  // 0
        let f1 = fibonacci(1);  // 1
        let f2 = fibonacci(2);  // 1
        let f3 = fibonacci(3);  // 2
        let f4 = fibonacci(4);  // 3
        let f5 = fibonacci(5);  // 5
        let f6 = fibonacci(6);  // 8

        f0 + f1 + f2 + f3 + f4 + f5 + f6;
    "#;
    assert_eq!(run(source), Value::Integer(20)); // 0+1+1+2+3+5+8
}

// === Additional Integration Tests ===

#[test]
fn test_complex_expression_precedence() {
    // Test operator precedence: 2 + 3 * 4 should be 14 (not 20)
    let source = "2 + 3 * 4;";
    assert_eq!(run(source), Value::Integer(14));

    // With parentheses: (2 + 3) * 4 should be 20
    let source = "(2 + 3) * 4;";
    assert_eq!(run(source), Value::Integer(20));
}

#[test]
fn test_nested_function_calls() {
    let source = r#"
        fn double(x) {
            return x * 2;
        }
        fn triple(x) {
            return x * 3;
        }
        double(triple(5));
    "#;
    assert_eq!(run(source), Value::Integer(30));
}

#[test]
fn test_scope_isolation() {
    let source = r#"
        let x = 10;
        if (true) {
            let x = 20;  // This shadows outer x
        }
        x;  // Should still be 10
    "#;
    assert_eq!(run(source), Value::Integer(10));
}

#[test]
fn test_builtin_len() {
    let source = r#"
        let s = "hello";
        len(s);
    "#;
    assert_eq!(run(source), Value::Integer(5));
}

#[test]
fn test_builtin_str() {
    let source = r#"
        let n = 42;
        str(n);
    "#;
    assert_eq!(run(source), Value::String("42".to_string()));
}

#[test]
fn test_builtin_int() {
    let source = r#"
        let s = "42";
        int(s);
    "#;
    assert_eq!(run(source), Value::Integer(42));
}

#[test]
fn test_else_if_chain() {
    let source = r#"
        let x = 5;
        let result = "";
        if (x < 0) {
            result = "negative";
        } else if (x == 0) {
            result = "zero";
        } else if (x < 10) {
            result = "small";
        } else {
            result = "large";
        }
        result;
    "#;
    assert_eq!(run(source), Value::String("small".to_string()));
}

#[test]
fn test_unary_operators() {
    let source = "-42;";
    assert_eq!(run(source), Value::Integer(-42));

    let source = "!true;";
    assert_eq!(run(source), Value::Boolean(false));

    let source = "!false;";
    assert_eq!(run(source), Value::Boolean(true));

    let source = "--5;";
    assert_eq!(run(source), Value::Integer(5));
}

#[test]
fn test_string_escapes() {
    let source = r#""hello\nworld";"#;
    assert_eq!(run(source), Value::String("hello\nworld".to_string()));

    let source = r#""tab\there";"#;
    assert_eq!(run(source), Value::String("tab\there".to_string()));
}

#[test]
fn test_modulo_operator() {
    let source = "17 % 5;";
    assert_eq!(run(source), Value::Integer(2));

    let source = "10 % 3;";
    assert_eq!(run(source), Value::Integer(1));
}

#[test]
fn test_comparison_operators() {
    assert_eq!(run("5 < 10;"), Value::Boolean(true));
    assert_eq!(run("5 > 10;"), Value::Boolean(false));
    assert_eq!(run("5 <= 5;"), Value::Boolean(true));
    assert_eq!(run("5 >= 5;"), Value::Boolean(true));
    assert_eq!(run("5 == 5;"), Value::Boolean(true));
    assert_eq!(run("5 != 5;"), Value::Boolean(false));
}

#[test]
fn test_function_no_return() {
    let source = r#"
        fn greet() {
            let x = 1;
        }
        greet();
    "#;
    assert_eq!(run(source), Value::Null);
}

#[test]
fn test_for_loop_with_no_init() {
    let source = r#"
        let i = 0;
        let sum = 0;
        for (; i < 3; i = i + 1) {
            sum = sum + i;
        }
        sum;
    "#;
    assert_eq!(run(source), Value::Integer(3)); // 0 + 1 + 2
}

// === Null and drop() tests ===

#[test]
fn test_null_literal() {
    assert_eq!(run("null;"), Value::Null);
}

#[test]
fn test_null_in_variable() {
    assert_eq!(run("let x = null; x;"), Value::Null);
}

#[test]
fn test_assign_to_null() {
    let source = r#"
        let text = "something";
        text = null;
        text;
    "#;
    assert_eq!(run(source), Value::Null);
}

#[test]
fn test_drop_function() {
    let source = r#"
        let text = "something";
        text = drop(text);
        text;
    "#;
    assert_eq!(run(source), Value::Null);
}

#[test]
fn test_null_equality() {
    assert_eq!(run("null == null;"), Value::Boolean(true));
    assert_eq!(run("null != 42;"), Value::Boolean(true));
    assert_eq!(run("null != \"hello\";"), Value::Boolean(true));
}

#[test]
fn test_null_in_conditional() {
    // null is falsy
    let source = r#"
        let x = null;
        if (x) {
            1;
        } else {
            0;
        }
    "#;
    assert_eq!(run(source), Value::Integer(0));
}

#[test]
fn test_drop_with_different_types() {
    assert_eq!(run("drop(42);"), Value::Null);
    assert_eq!(run("drop(\"hello\");"), Value::Null);
    assert_eq!(run("drop(true);"), Value::Null);
    assert_eq!(run("drop(null);"), Value::Null);
}
