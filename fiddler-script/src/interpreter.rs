//! Interpreter for FiddlerScript.
//!
//! This module contains the execution engine that evaluates the AST and
//! produces runtime values.

use std::collections::HashMap;

use indexmap::IndexMap;

use crate::ast::{BinaryOp, Block, ElseClause, Expression, Program, Statement, UnaryOp};
use crate::builtins::get_default_builtins;
use crate::error::RuntimeError;
use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::Value;

/// Maximum recursion depth to prevent stack overflow.
/// Set conservatively to avoid hitting the actual OS stack limit,
/// especially in test environments with smaller thread stacks.
const MAX_CALL_DEPTH: usize = 64;

/// Maximum number of print output lines to capture before truncation.
const MAX_OUTPUT_LINES: usize = 1000;

/// Maximum total bytes of print output to capture before truncation.
const MAX_OUTPUT_BYTES: usize = 64 * 1024; // 64 KB

/// A user-defined function.
#[derive(Debug, Clone)]
struct UserFunction {
    /// Parameter names
    params: Vec<String>,
    /// Function body
    body: Block,
}

/// Control flow signal for early returns.
enum ControlFlow {
    /// Normal execution continues
    Continue(Value),
    /// Return from function with value
    Return(Value),
}

impl ControlFlow {
    fn into_value(self) -> Value {
        match self {
            ControlFlow::Continue(v) | ControlFlow::Return(v) => v,
        }
    }
}

/// The FiddlerScript interpreter.
pub struct Interpreter {
    /// Stack of variable scopes (environment)
    scopes: Vec<HashMap<String, Value>>,
    /// User-defined functions
    functions: HashMap<String, UserFunction>,
    /// Built-in functions
    builtins: HashMap<String, fn(Vec<Value>) -> Result<Value, RuntimeError>>,
    /// Output capture (for testing)
    output: Vec<String>,
    /// Total bytes captured in output
    output_bytes: usize,
    /// Whether output has been truncated
    output_truncated: bool,
    /// Current call depth for recursion limiting
    call_depth: usize,
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

impl Interpreter {
    /// Create a new interpreter with default built-in functions.
    ///
    /// The interpreter is initialized with all current OS environment variables
    /// available as script variables.
    pub fn new() -> Self {
        Self::with_builtins(get_default_builtins())
    }

    /// Create a new interpreter with custom built-in functions.
    ///
    /// The interpreter is initialized with all current OS environment variables
    /// available as script variables.
    pub fn with_builtins(
        builtins: HashMap<String, fn(Vec<Value>) -> Result<Value, RuntimeError>>,
    ) -> Self {
        let mut global_scope = HashMap::new();

        // Load all OS environment variables into the global scope
        for (key, value) in std::env::vars() {
            global_scope.insert(key, Value::String(value));
        }

        Self {
            scopes: vec![global_scope],
            functions: HashMap::new(),
            builtins,
            output: Vec::new(),
            output_bytes: 0,
            output_truncated: false,
            call_depth: 0,
        }
    }

    /// Create a new interpreter without loading environment variables.
    pub fn new_without_env() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            functions: HashMap::new(),
            builtins: get_default_builtins(),
            output: Vec::new(),
            output_bytes: 0,
            output_truncated: false,
            call_depth: 0,
        }
    }

    /// Run FiddlerScript source code.
    pub fn run(&mut self, source: &str) -> Result<Value, crate::FiddlerError> {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize()?;
        let mut parser = Parser::new(tokens);
        let program = parser.parse()?;
        Ok(self.execute(&program)?)
    }

    /// Execute a parsed program.
    pub fn execute(&mut self, program: &Program) -> Result<Value, RuntimeError> {
        let mut result = Value::Null;
        for statement in &program.statements {
            match self.execute_statement(statement)? {
                ControlFlow::Continue(v) => result = v,
                ControlFlow::Return(_) => {
                    return Err(RuntimeError::ReturnOutsideFunction);
                }
            }
        }
        Ok(result)
    }

    /// Get captured output (for testing).
    pub fn output(&self) -> &[String] {
        &self.output
    }

    /// Clear captured output.
    pub fn clear_output(&mut self) {
        self.output.clear();
        self.output_bytes = 0;
        self.output_truncated = false;
    }

    /// Check if captured output was truncated.
    pub fn is_output_truncated(&self) -> bool {
        self.output_truncated
    }

    // === Public variable access API ===

    /// Set a variable by name in the global scope.
    ///
    /// This allows external code to inject values into the interpreter
    /// before running a script.
    ///
    /// # Arguments
    /// * `name` - The variable name
    /// * `value` - The value to set
    pub fn set_variable_value(&mut self, name: impl Into<String>, value: Value) {
        if let Some(scope) = self.scopes.first_mut() {
            scope.insert(name.into(), value);
        }
    }

    /// Set a variable from bytes.
    ///
    /// Convenience method to set a variable with byte data.
    ///
    /// # Arguments
    /// * `name` - The variable name
    /// * `bytes` - The byte data
    pub fn set_variable_bytes(&mut self, name: impl Into<String>, bytes: Vec<u8>) {
        self.set_variable_value(name, Value::Bytes(bytes));
    }

    /// Set a variable from a string.
    ///
    /// Convenience method to set a variable with string data.
    ///
    /// # Arguments
    /// * `name` - The variable name
    /// * `value` - The string value
    pub fn set_variable_string(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.set_variable_value(name, Value::String(value.into()));
    }

    /// Set a variable from an integer.
    ///
    /// Convenience method to set a variable with an integer value.
    ///
    /// # Arguments
    /// * `name` - The variable name
    /// * `value` - The integer value
    pub fn set_variable_int(&mut self, name: impl Into<String>, value: i64) {
        self.set_variable_value(name, Value::Integer(value));
    }

    /// Set a variable as an array.
    ///
    /// Convenience method to set a variable with an array value.
    ///
    /// # Arguments
    /// * `name` - The variable name
    /// * `values` - The array values
    pub fn set_variable_array(&mut self, name: impl Into<String>, values: Vec<Value>) {
        self.set_variable_value(name, Value::Array(values));
    }

    /// Set a variable as a dictionary.
    ///
    /// Convenience method to set a variable with a dictionary value.
    ///
    /// # Arguments
    /// * `name` - The variable name
    /// * `values` - The dictionary values
    pub fn set_variable_dict(&mut self, name: impl Into<String>, values: IndexMap<String, Value>) {
        self.set_variable_value(name, Value::Dictionary(values));
    }

    /// Get a variable by name, returning the Value type.
    ///
    /// Searches all scopes from innermost to outermost.
    ///
    /// # Arguments
    /// * `name` - The variable name
    ///
    /// # Returns
    /// * `Some(Value)` if the variable exists
    /// * `None` if the variable is not defined
    pub fn get_value(&self, name: &str) -> Option<Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(value) = scope.get(name) {
                return Some(value.clone());
            }
        }
        None
    }

    /// Get a variable by name as bytes.
    ///
    /// Converts the variable value to its byte representation.
    ///
    /// # Arguments
    /// * `name` - The variable name
    ///
    /// # Returns
    /// * `Some(Vec<u8>)` if the variable exists
    /// * `None` if the variable is not defined
    pub fn get_bytes(&self, name: &str) -> Option<Vec<u8>> {
        self.get_value(name).map(|v| v.to_bytes())
    }

    /// Check if a variable exists.
    ///
    /// # Arguments
    /// * `name` - The variable name
    ///
    /// # Returns
    /// * `true` if the variable exists in any scope
    /// * `false` otherwise
    pub fn has_variable(&self, name: &str) -> bool {
        self.get_value(name).is_some()
    }

    // === Scope management ===

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn define_variable(&mut self, name: String, value: Value) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, value);
        }
    }

    fn get_variable(&self, name: &str) -> Result<Value, RuntimeError> {
        for scope in self.scopes.iter().rev() {
            if let Some(value) = scope.get(name) {
                return Ok(value.clone());
            }
        }
        Err(RuntimeError::undefined_variable(name))
    }

    fn set_variable(&mut self, name: &str, value: Value) -> Result<(), RuntimeError> {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), value);
                return Ok(());
            }
        }
        Err(RuntimeError::undefined_variable(name))
    }

    // === Statement execution ===

    fn execute_statement(&mut self, stmt: &Statement) -> Result<ControlFlow, RuntimeError> {
        match stmt {
            Statement::Let { name, value, .. } => {
                let val = self.evaluate_expression(value)?;
                self.define_variable(name.clone(), val);
                Ok(ControlFlow::Continue(Value::Null))
            }

            Statement::If {
                condition,
                then_block,
                else_block,
                ..
            } => {
                let cond = self.evaluate_expression(condition)?;
                if self.is_truthy(&cond) {
                    self.execute_block(then_block)
                } else if let Some(else_clause) = else_block {
                    match else_clause {
                        ElseClause::Block(block) => self.execute_block(block),
                        ElseClause::ElseIf(if_stmt) => self.execute_statement(if_stmt),
                    }
                } else {
                    Ok(ControlFlow::Continue(Value::Null))
                }
            }

            Statement::For {
                init,
                condition,
                update,
                body,
                ..
            } => {
                self.push_scope();

                // Execute init
                if let Some(init_stmt) = init {
                    self.execute_statement(init_stmt)?;
                }

                // Loop
                loop {
                    // Check condition
                    if let Some(cond) = condition {
                        let cond_val = self.evaluate_expression(cond)?;
                        if !self.is_truthy(&cond_val) {
                            break;
                        }
                    }

                    // Execute body
                    match self.execute_block(body)? {
                        ControlFlow::Return(v) => {
                            self.pop_scope();
                            return Ok(ControlFlow::Return(v));
                        }
                        ControlFlow::Continue(_) => {}
                    }

                    // Execute update
                    if let Some(upd) = update {
                        self.evaluate_expression(upd)?;
                    }
                }

                self.pop_scope();
                Ok(ControlFlow::Continue(Value::Null))
            }

            Statement::Return { value, .. } => {
                let val = if let Some(expr) = value {
                    self.evaluate_expression(expr)?
                } else {
                    Value::Null
                };
                Ok(ControlFlow::Return(val))
            }

            Statement::Function {
                name, params, body, ..
            } => {
                self.functions.insert(
                    name.clone(),
                    UserFunction {
                        params: params.clone(),
                        body: body.clone(),
                    },
                );
                Ok(ControlFlow::Continue(Value::Null))
            }

            Statement::Expression { expression, .. } => {
                let val = self.evaluate_expression(expression)?;
                Ok(ControlFlow::Continue(val))
            }

            Statement::Block(block) => self.execute_block(block),
        }
    }

    fn execute_block(&mut self, block: &Block) -> Result<ControlFlow, RuntimeError> {
        self.push_scope();
        let mut result = ControlFlow::Continue(Value::Null);

        for stmt in &block.statements {
            result = self.execute_statement(stmt)?;
            if matches!(result, ControlFlow::Return(_)) {
                break;
            }
        }

        self.pop_scope();
        Ok(result)
    }

    // === Expression evaluation ===

    fn evaluate_expression(&mut self, expr: &Expression) -> Result<Value, RuntimeError> {
        match expr {
            Expression::Integer { value, .. } => Ok(Value::Integer(*value)),
            Expression::Float { value, .. } => Ok(Value::Float(*value)),
            Expression::String { value, .. } => Ok(Value::String(value.clone())),
            Expression::Boolean { value, .. } => Ok(Value::Boolean(*value)),

            Expression::Identifier { name, .. } => self.get_variable(name),

            Expression::Binary {
                left,
                operator,
                right,
                ..
            } => {
                let left_val = self.evaluate_expression(left)?;
                let right_val = self.evaluate_expression(right)?;
                self.evaluate_binary_op(*operator, left_val, right_val)
            }

            Expression::Unary {
                operator, operand, ..
            } => {
                let val = self.evaluate_expression(operand)?;
                self.evaluate_unary_op(*operator, val)
            }

            Expression::Assignment { name, value, .. } => {
                let val = self.evaluate_expression(value)?;
                self.set_variable(name, val.clone())?;
                Ok(val)
            }

            Expression::Call {
                function,
                arguments,
                ..
            } => self.call_function(function, arguments),

            Expression::MethodCall {
                receiver,
                method,
                arguments,
                ..
            } => self.call_method(receiver, method, arguments),

            Expression::Grouped { expression, .. } => self.evaluate_expression(expression),

            Expression::ArrayLiteral { elements, .. } => {
                let values: Vec<Value> = elements
                    .iter()
                    .map(|e| self.evaluate_expression(e))
                    .collect::<Result<_, _>>()?;
                Ok(Value::Array(values))
            }

            Expression::DictionaryLiteral { pairs, .. } => {
                let mut dict = IndexMap::new();
                for (key_expr, value_expr) in pairs {
                    let key = match self.evaluate_expression(key_expr)? {
                        Value::String(s) => s,
                        _ => {
                            return Err(RuntimeError::type_mismatch(
                                "Dictionary key must be a string",
                            ))
                        }
                    };
                    let value = self.evaluate_expression(value_expr)?;
                    dict.insert(key, value);
                }
                Ok(Value::Dictionary(dict))
            }
        }
    }

    fn evaluate_binary_op(
        &mut self,
        op: BinaryOp,
        left: Value,
        right: Value,
    ) -> Result<Value, RuntimeError> {
        match op {
            // Arithmetic operations with float support
            BinaryOp::Add => match (&left, &right) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a + b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
                (Value::Integer(a), Value::Float(b)) => Ok(Value::Float(*a as f64 + b)),
                (Value::Float(a), Value::Integer(b)) => Ok(Value::Float(a + *b as f64)),
                (Value::String(a), Value::String(b)) => Ok(Value::String(format!("{}{}", a, b))),
                _ => Err(RuntimeError::type_mismatch(format!(
                    "Cannot add {:?} and {:?}",
                    left, right
                ))),
            },
            BinaryOp::Subtract => match (&left, &right) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a - b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
                (Value::Integer(a), Value::Float(b)) => Ok(Value::Float(*a as f64 - b)),
                (Value::Float(a), Value::Integer(b)) => Ok(Value::Float(a - *b as f64)),
                _ => Err(RuntimeError::type_mismatch(
                    "Subtraction requires numeric types".to_string(),
                )),
            },
            BinaryOp::Multiply => match (&left, &right) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a * b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
                (Value::Integer(a), Value::Float(b)) => Ok(Value::Float(*a as f64 * b)),
                (Value::Float(a), Value::Integer(b)) => Ok(Value::Float(a * *b as f64)),
                _ => Err(RuntimeError::type_mismatch(
                    "Multiplication requires numeric types".to_string(),
                )),
            },
            BinaryOp::Divide => match (&left, &right) {
                (Value::Integer(_), Value::Integer(0)) => {
                    Err(RuntimeError::DivisionByZero { position: None })
                }
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a / b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)), // IEEE 754: /0 = Inf
                (Value::Integer(a), Value::Float(b)) => Ok(Value::Float(*a as f64 / b)),
                (Value::Float(a), Value::Integer(b)) => Ok(Value::Float(a / *b as f64)),
                _ => Err(RuntimeError::type_mismatch(
                    "Division requires numeric types".to_string(),
                )),
            },
            BinaryOp::Modulo => match (&left, &right) {
                (Value::Integer(_), Value::Integer(0)) => {
                    Err(RuntimeError::DivisionByZero { position: None })
                }
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a % b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a % b)),
                (Value::Integer(a), Value::Float(b)) => Ok(Value::Float(*a as f64 % b)),
                (Value::Float(a), Value::Integer(b)) => Ok(Value::Float(a % *b as f64)),
                _ => Err(RuntimeError::type_mismatch(
                    "Modulo requires numeric types".to_string(),
                )),
            },

            // Comparison
            BinaryOp::Equal => Ok(Value::Boolean(left == right)),
            BinaryOp::NotEqual => Ok(Value::Boolean(left != right)),
            BinaryOp::LessThan
            | BinaryOp::LessEqual
            | BinaryOp::GreaterThan
            | BinaryOp::GreaterEqual => self.evaluate_comparison(op, &left, &right),

            // Logical
            BinaryOp::And => Ok(Value::Boolean(
                self.is_truthy(&left) && self.is_truthy(&right),
            )),
            BinaryOp::Or => Ok(Value::Boolean(
                self.is_truthy(&left) || self.is_truthy(&right),
            )),
        }
    }

    /// Evaluate comparison operators (<, <=, >, >=).
    fn evaluate_comparison(
        &self,
        op: BinaryOp,
        left: &Value,
        right: &Value,
    ) -> Result<Value, RuntimeError> {
        let result = match (left, right) {
            (Value::Integer(a), Value::Integer(b)) => match op {
                BinaryOp::LessThan => a < b,
                BinaryOp::LessEqual => a <= b,
                BinaryOp::GreaterThan => a > b,
                BinaryOp::GreaterEqual => a >= b,
                _ => unreachable!(),
            },
            (Value::Float(a), Value::Float(b)) => match op {
                BinaryOp::LessThan => a < b,
                BinaryOp::LessEqual => a <= b,
                BinaryOp::GreaterThan => a > b,
                BinaryOp::GreaterEqual => a >= b,
                _ => unreachable!(),
            },
            // Mixed numeric comparisons (promote integer to float)
            (Value::Integer(a), Value::Float(b)) => {
                let a_float = *a as f64;
                match op {
                    BinaryOp::LessThan => a_float < *b,
                    BinaryOp::LessEqual => a_float <= *b,
                    BinaryOp::GreaterThan => a_float > *b,
                    BinaryOp::GreaterEqual => a_float >= *b,
                    _ => unreachable!(),
                }
            }
            (Value::Float(a), Value::Integer(b)) => {
                let b_float = *b as f64;
                match op {
                    BinaryOp::LessThan => *a < b_float,
                    BinaryOp::LessEqual => *a <= b_float,
                    BinaryOp::GreaterThan => *a > b_float,
                    BinaryOp::GreaterEqual => *a >= b_float,
                    _ => unreachable!(),
                }
            }
            (Value::String(a), Value::String(b)) => match op {
                BinaryOp::LessThan => a < b,
                BinaryOp::LessEqual => a <= b,
                BinaryOp::GreaterThan => a > b,
                BinaryOp::GreaterEqual => a >= b,
                _ => unreachable!(),
            },
            _ => {
                return Err(RuntimeError::type_mismatch(
                    "Comparison requires matching or numeric types".to_string(),
                ))
            }
        };
        Ok(Value::Boolean(result))
    }

    fn evaluate_unary_op(&self, op: UnaryOp, operand: Value) -> Result<Value, RuntimeError> {
        match op {
            UnaryOp::Not => Ok(Value::Boolean(!self.is_truthy(&operand))),
            UnaryOp::Negate => match operand {
                Value::Integer(n) => Ok(Value::Integer(-n)),
                Value::Float(f) => Ok(Value::Float(-f)),
                _ => Err(RuntimeError::type_mismatch(
                    "Negation requires numeric type".to_string(),
                )),
            },
        }
    }

    fn is_truthy(&self, value: &Value) -> bool {
        match value {
            Value::Boolean(b) => *b,
            Value::Integer(n) => *n != 0,
            Value::Float(f) => *f != 0.0 && !f.is_nan(),
            Value::String(s) => !s.is_empty(),
            Value::Bytes(b) => !b.is_empty(),
            Value::Array(a) => !a.is_empty(),
            Value::Dictionary(d) => !d.is_empty(),
            Value::Null => false,
        }
    }

    /// Capture output for the print function with truncation support.
    ///
    /// This is used for testing to capture print output. Output is truncated
    /// if it exceeds MAX_OUTPUT_LINES or MAX_OUTPUT_BYTES.
    fn capture_print_output(&mut self, args: &[Value]) {
        if self.output_truncated {
            return;
        }

        let output_str = args
            .iter()
            .map(|v| format!("{}", v))
            .collect::<Vec<_>>()
            .join(" ");

        let new_bytes = self.output_bytes + output_str.len();
        let new_lines = self.output.len() + 1;

        if new_lines > MAX_OUTPUT_LINES || new_bytes > MAX_OUTPUT_BYTES {
            self.output.push("[truncated]".to_string());
            self.output_truncated = true;
        } else {
            self.output_bytes = new_bytes;
            self.output.push(output_str);
        }
    }

    /// Call a built-in or user-defined function by name.
    fn call_function(
        &mut self,
        name: &str,
        arguments: &[Expression],
    ) -> Result<Value, RuntimeError> {
        // Evaluate arguments
        let args: Vec<Value> = arguments
            .iter()
            .map(|arg| self.evaluate_expression(arg))
            .collect::<Result<_, _>>()?;

        // Check for built-in function (copy the fn pointer to avoid borrow issues)
        if let Some(&builtin) = self.builtins.get(name) {
            if name == "print" {
                self.capture_print_output(&args);
            }
            return builtin(args);
        }

        // Check for user-defined function
        if let Some(func) = self.functions.get(name).cloned() {
            return self.call_user_function(&func, args);
        }

        Err(RuntimeError::undefined_function(name))
    }

    /// Call a method on a receiver expression.
    ///
    /// This implements method call syntax as syntactic sugar over function calls.
    /// Method calls like `receiver.method(arg1, arg2)` are transformed into regular
    /// function calls with the receiver prepended: `method(receiver, arg1, arg2)`.
    ///
    /// # Examples
    ///
    /// ```text
    /// "hello".len()        -> len("hello")
    /// arr.push(42)         -> push(arr, 42)
    /// "hi".split(",")      -> split("hi", ",")
    /// ```
    ///
    /// # Arguments
    ///
    /// * `receiver` - The expression before the dot (e.g., `"hello"` in `"hello".len()`)
    /// * `method` - The method name to call (e.g., `"len"`)
    /// * `arguments` - The arguments passed to the method (empty for `len()`)
    ///
    /// # Returns
    ///
    /// Returns the result of calling the builtin or user-defined function with
    /// the receiver prepended to the argument list.
    ///
    /// # Errors
    ///
    /// Returns `RuntimeError::UndefinedFunction` if no builtin or user function
    /// with the given method name exists.
    fn call_method(
        &mut self,
        receiver: &Expression,
        method: &str,
        arguments: &[Expression],
    ) -> Result<Value, RuntimeError> {
        // Evaluate receiver first
        let receiver_value = self.evaluate_expression(receiver)?;

        // Pre-allocate with correct capacity and build args in order
        // This avoids the O(n) cost of insert(0, ...)
        let mut args = Vec::with_capacity(arguments.len() + 1);
        args.push(receiver_value);

        for arg in arguments {
            args.push(self.evaluate_expression(arg)?);
        }

        // Check for built-in function (copy the fn pointer to avoid borrow issues)
        if let Some(&builtin) = self.builtins.get(method) {
            if method == "print" {
                self.capture_print_output(&args);
            }
            return builtin(args);
        }

        // Check for user-defined function
        if let Some(func) = self.functions.get(method).cloned() {
            return self.call_user_function(&func, args);
        }

        Err(RuntimeError::undefined_function(method))
    }

    /// Call a user-defined function with pre-evaluated arguments.
    fn call_user_function(
        &mut self,
        func: &UserFunction,
        args: Vec<Value>,
    ) -> Result<Value, RuntimeError> {
        // Check recursion depth
        if self.call_depth >= MAX_CALL_DEPTH {
            return Err(RuntimeError::StackOverflow {
                max_depth: MAX_CALL_DEPTH,
            });
        }

        if args.len() != func.params.len() {
            return Err(RuntimeError::WrongArgumentCount {
                expected: func.params.len(),
                actual: args.len(),
                position: None,
            });
        }

        // Increment call depth
        self.call_depth += 1;

        // Create new scope for function
        self.push_scope();

        // Bind arguments to parameters
        for (param, arg) in func.params.iter().zip(args) {
            self.define_variable(param.clone(), arg);
        }

        // Execute function body
        let result = self.execute_block(&func.body);

        self.pop_scope();

        // Decrement call depth (even on error)
        self.call_depth -= 1;

        result.map(|cf| cf.into_value())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Result<Value, crate::FiddlerError> {
        let mut interpreter = Interpreter::new();
        interpreter.run(source)
    }

    #[test]
    fn test_integer_literal() {
        assert_eq!(run("42;").unwrap(), Value::Integer(42));
    }

    #[test]
    fn test_string_literal() {
        assert_eq!(
            run(r#""hello";"#).unwrap(),
            Value::String("hello".to_string())
        );
    }

    #[test]
    fn test_boolean_literal() {
        assert_eq!(run("true;").unwrap(), Value::Boolean(true));
        assert_eq!(run("false;").unwrap(), Value::Boolean(false));
    }

    #[test]
    fn test_arithmetic() {
        assert_eq!(run("5 + 3;").unwrap(), Value::Integer(8));
        assert_eq!(run("5 - 3;").unwrap(), Value::Integer(2));
        assert_eq!(run("5 * 3;").unwrap(), Value::Integer(15));
        assert_eq!(run("6 / 2;").unwrap(), Value::Integer(3));
        assert_eq!(run("7 % 3;").unwrap(), Value::Integer(1));
    }

    #[test]
    fn test_string_concatenation() {
        assert_eq!(
            run(r#""hello" + " " + "world";"#).unwrap(),
            Value::String("hello world".to_string())
        );
    }

    #[test]
    fn test_comparison() {
        assert_eq!(run("5 > 3;").unwrap(), Value::Boolean(true));
        assert_eq!(run("5 < 3;").unwrap(), Value::Boolean(false));
        assert_eq!(run("5 == 5;").unwrap(), Value::Boolean(true));
        assert_eq!(run("5 != 3;").unwrap(), Value::Boolean(true));
    }

    #[test]
    fn test_logical() {
        assert_eq!(run("true && true;").unwrap(), Value::Boolean(true));
        assert_eq!(run("true && false;").unwrap(), Value::Boolean(false));
        assert_eq!(run("true || false;").unwrap(), Value::Boolean(true));
        assert_eq!(run("!true;").unwrap(), Value::Boolean(false));
    }

    #[test]
    fn test_variable() {
        assert_eq!(run("let x = 10; x;").unwrap(), Value::Integer(10));
    }

    #[test]
    fn test_variable_assignment() {
        assert_eq!(run("let x = 10; x = 20; x;").unwrap(), Value::Integer(20));
    }

    #[test]
    fn test_if_true() {
        assert_eq!(
            run("let x = 0; if (true) { x = 1; } x;").unwrap(),
            Value::Integer(1)
        );
    }

    #[test]
    fn test_if_false() {
        assert_eq!(
            run("let x = 0; if (false) { x = 1; } x;").unwrap(),
            Value::Integer(0)
        );
    }

    #[test]
    fn test_if_else() {
        assert_eq!(
            run("let x = 0; if (false) { x = 1; } else { x = 2; } x;").unwrap(),
            Value::Integer(2)
        );
    }

    #[test]
    fn test_for_loop() {
        assert_eq!(
            run("let sum = 0; for (let i = 0; i < 5; i = i + 1) { sum = sum + i; } sum;").unwrap(),
            Value::Integer(10) // 0 + 1 + 2 + 3 + 4
        );
    }

    #[test]
    fn test_function() {
        assert_eq!(
            run("fn add(a, b) { return a + b; } add(2, 3);").unwrap(),
            Value::Integer(5)
        );
    }

    #[test]
    fn test_recursion() {
        let source = r#"
            fn factorial(n) {
                if (n <= 1) {
                    return 1;
                }
                return n * factorial(n - 1);
            }
            factorial(5);
        "#;
        assert_eq!(run(source).unwrap(), Value::Integer(120));
    }

    #[test]
    fn test_division_by_zero() {
        let result = run("5 / 0;");
        assert!(matches!(
            result,
            Err(crate::FiddlerError::Runtime(RuntimeError::DivisionByZero {
                position: None
            }))
        ));
    }

    #[test]
    fn test_undefined_variable() {
        let result = run("x;");
        assert!(matches!(
            result,
            Err(crate::FiddlerError::Runtime(
                RuntimeError::UndefinedVariable { .. }
            ))
        ));
    }

    #[test]
    fn test_undefined_function() {
        let result = run("foo();");
        assert!(matches!(
            result,
            Err(crate::FiddlerError::Runtime(
                RuntimeError::UndefinedFunction { .. }
            ))
        ));
    }

    #[test]
    fn test_wrong_argument_count() {
        let result = run("fn add(a, b) { return a + b; } add(1);");
        assert!(matches!(
            result,
            Err(crate::FiddlerError::Runtime(
                RuntimeError::WrongArgumentCount {
                    expected: 2,
                    actual: 1,
                    ..
                }
            ))
        ));
    }

    #[test]
    fn test_set_and_get_variable() {
        let mut interpreter = Interpreter::new_without_env();
        interpreter.set_variable_value("x", Value::Integer(42));
        assert_eq!(interpreter.get_value("x"), Some(Value::Integer(42)));
    }

    #[test]
    fn test_set_variable_string() {
        let mut interpreter = Interpreter::new_without_env();
        interpreter.set_variable_string("name", "Alice");
        assert_eq!(
            interpreter.get_value("name"),
            Some(Value::String("Alice".to_string()))
        );
    }

    #[test]
    fn test_set_variable_bytes() {
        let mut interpreter = Interpreter::new_without_env();
        interpreter.set_variable_bytes("data", vec![1, 2, 3]);
        assert_eq!(
            interpreter.get_value("data"),
            Some(Value::Bytes(vec![1, 2, 3]))
        );
    }

    #[test]
    fn test_get_bytes() {
        let mut interpreter = Interpreter::new_without_env();
        interpreter.set_variable_string("msg", "hello");
        assert_eq!(
            interpreter.get_bytes("msg"),
            Some("hello".as_bytes().to_vec())
        );
    }

    #[test]
    fn test_has_variable() {
        let mut interpreter = Interpreter::new_without_env();
        interpreter.set_variable_int("count", 10);
        assert!(interpreter.has_variable("count"));
        assert!(!interpreter.has_variable("nonexistent"));
    }

    #[test]
    fn test_env_vars_loaded() {
        std::env::set_var("FIDDLER_TEST_VAR", "test123");
        let interpreter = Interpreter::new();
        assert_eq!(
            interpreter.get_value("FIDDLER_TEST_VAR"),
            Some(Value::String("test123".to_string()))
        );
        std::env::remove_var("FIDDLER_TEST_VAR");
    }

    #[test]
    fn test_use_injected_variable_in_script() {
        let mut interpreter = Interpreter::new_without_env();
        interpreter.set_variable_int("input", 5);
        let result = interpreter.run("input * 2;").unwrap();
        assert_eq!(result, Value::Integer(10));
    }

    #[test]
    fn test_bytes_in_script() {
        let mut interpreter = Interpreter::new_without_env();
        interpreter.set_variable_bytes("data", b"hello".to_vec());
        let result = interpreter.run("bytes_to_string(data);").unwrap();
        assert_eq!(result, Value::String("hello".to_string()));
    }

    #[test]
    fn test_parse_json_in_script() {
        let mut interpreter = Interpreter::new_without_env();
        interpreter.set_variable_bytes("json_data", br#"{"name": "test"}"#.to_vec());
        let result = interpreter.run("parse_json(json_data);").unwrap();
        assert!(matches!(result, Value::Dictionary(_)));
    }

    #[test]
    fn test_bytes_truthy() {
        let mut interpreter = Interpreter::new_without_env();
        interpreter.set_variable_bytes("data", b"hello".to_vec());
        let result = interpreter.run("if (data) { 1; } else { 0; }").unwrap();
        assert_eq!(result, Value::Integer(1));
    }

    #[test]
    fn test_empty_bytes_falsy() {
        let mut interpreter = Interpreter::new_without_env();
        interpreter.set_variable_bytes("empty", vec![]);
        let result = interpreter.run("if (empty) { 1; } else { 0; }").unwrap();
        assert_eq!(result, Value::Integer(0));
    }

    #[test]
    fn test_print_output_capture() {
        let mut interpreter = Interpreter::new_without_env();
        interpreter
            .run(r#"print("hello"); print("world");"#)
            .unwrap();
        assert_eq!(interpreter.output(), &["hello", "world"]);
    }

    #[test]
    fn test_print_output_multiple_args() {
        let mut interpreter = Interpreter::new_without_env();
        interpreter.run(r#"print("a", 42, true);"#).unwrap();
        assert_eq!(interpreter.output(), &["a 42 true"]);
    }

    #[test]
    fn test_clear_output() {
        let mut interpreter = Interpreter::new_without_env();
        interpreter.run(r#"print("test");"#).unwrap();
        assert_eq!(interpreter.output().len(), 1);
        assert!(!interpreter.is_output_truncated());
        interpreter.clear_output();
        assert!(interpreter.output().is_empty());
        assert!(!interpreter.is_output_truncated());
    }

    #[test]
    fn test_output_truncation_by_lines() {
        let mut interpreter = Interpreter::new_without_env();
        // Generate more than MAX_OUTPUT_LINES (1000) print statements
        let mut source = String::new();
        for i in 0..1005 {
            source.push_str(&format!("print({});\n", i));
        }
        interpreter.run(&source).unwrap();

        // Should have MAX_OUTPUT_LINES entries plus "[truncated]"
        assert!(interpreter.output().len() <= MAX_OUTPUT_LINES + 1);
        assert!(interpreter.is_output_truncated());
        assert_eq!(
            interpreter.output().last(),
            Some(&"[truncated]".to_string())
        );
    }

    #[test]
    fn test_output_truncation_by_bytes() {
        let mut interpreter = Interpreter::new_without_env();
        // Generate a long string that exceeds MAX_OUTPUT_BYTES (64KB)
        let long_string = "x".repeat(70_000);
        let source = format!(r#"print("{}");"#, long_string);
        interpreter.run(&source).unwrap();

        // The single line exceeds the byte limit, so it should be truncated
        assert!(interpreter.is_output_truncated());
        assert_eq!(
            interpreter.output().last(),
            Some(&"[truncated]".to_string())
        );
    }

    #[test]
    fn test_output_not_truncated_within_limits() {
        let mut interpreter = Interpreter::new_without_env();
        // Just a few print statements should not trigger truncation
        interpreter
            .run(r#"print("a"); print("b"); print("c");"#)
            .unwrap();
        assert!(!interpreter.is_output_truncated());
        assert_eq!(interpreter.output(), &["a", "b", "c"]);
    }

    #[test]
    fn test_stack_overflow() {
        let mut interpreter = Interpreter::new_without_env();
        let source = r#"
            fn recurse() {
                recurse();
            }
            recurse();
        "#;
        let result = interpreter.run(source);
        assert!(matches!(
            result,
            Err(crate::FiddlerError::Runtime(
                RuntimeError::StackOverflow { .. }
            ))
        ));
    }

    #[test]
    fn test_deep_recursion_within_limit() {
        let mut interpreter = Interpreter::new_without_env();
        // Test that reasonable recursion depth works (50 levels, well under MAX_CALL_DEPTH of 64)
        let source = r#"
            fn count_down(n) {
                if (n <= 0) {
                    return 0;
                }
                return 1 + count_down(n - 1);
            }
            count_down(50);
        "#;
        let result = interpreter.run(source).unwrap();
        assert_eq!(result, Value::Integer(50));
    }

    #[test]
    fn test_array_literal() {
        let result = run("[1, 2, 3];").unwrap();
        assert_eq!(
            result,
            Value::Array(vec![
                Value::Integer(1),
                Value::Integer(2),
                Value::Integer(3),
            ])
        );
    }

    #[test]
    fn test_empty_array_literal() {
        let result = run("[];").unwrap();
        assert_eq!(result, Value::Array(vec![]));
    }

    #[test]
    fn test_dictionary_literal() {
        let result = run(r#"{"key": 42};"#).unwrap();
        if let Value::Dictionary(dict) = result {
            assert_eq!(dict.get("key"), Some(&Value::Integer(42)));
        } else {
            panic!("Expected dictionary");
        }
    }

    #[test]
    fn test_empty_dictionary_literal() {
        let result = run("{};").unwrap();
        assert_eq!(result, Value::Dictionary(IndexMap::new()));
    }

    #[test]
    fn test_nested_literals() {
        let result = run(r#"{"arr": [1, 2], "nested": {"a": 1}};"#).unwrap();
        assert!(matches!(result, Value::Dictionary(_)));
    }

    #[test]
    fn test_dictionary_preserves_insertion_order() {
        // Test that dictionary maintains insertion order (IndexMap behavior)
        let result = run(r#"{"z": 1, "a": 2, "m": 3};"#).unwrap();
        if let Value::Dictionary(dict) = result {
            let keys: Vec<&String> = dict.keys().collect();
            assert_eq!(keys, vec!["z", "a", "m"]);
        } else {
            panic!("Expected dictionary");
        }
    }

    #[test]
    fn test_keys_preserves_insertion_order() {
        let mut interpreter = Interpreter::new_without_env();
        let result = interpreter
            .run(r#"let d = {"third": 3, "first": 1, "second": 2}; keys(d);"#)
            .unwrap();
        // Keys should be in insertion order
        assert_eq!(
            result,
            Value::Array(vec![
                Value::String("third".to_string()),
                Value::String("first".to_string()),
                Value::String("second".to_string()),
            ])
        );
    }

    // Edge case tests (#15)

    #[test]
    fn test_max_integer() {
        let max = i64::MAX;
        let source = format!("{};", max);
        let result = run(&source).unwrap();
        assert_eq!(result, Value::Integer(max));
    }

    #[test]
    fn test_min_integer() {
        // Note: -9223372036854775808 would be parsed as negate(9223372036854775808)
        // which overflows, so we use a slightly different approach
        let min_plus_one = i64::MIN + 1;
        let source = format!("{};", min_plus_one);
        let result = run(&source).unwrap();
        assert_eq!(result, Value::Integer(min_plus_one));
    }

    #[test]
    fn test_unicode_strings() {
        // Test various Unicode characters
        let result = run(r#""Hello, 世界! 🎉 émojis";"#).unwrap();
        assert_eq!(result, Value::String("Hello, 世界! 🎉 émojis".to_string()));
    }

    #[test]
    fn test_unicode_string_concatenation() {
        let result = run(r#""こんにちは" + " " + "世界";"#).unwrap();
        assert_eq!(result, Value::String("こんにちは 世界".to_string()));
    }

    #[test]
    fn test_deeply_nested_expressions() {
        // Test deeply nested parenthesized expressions
        // (1 + 2) = 3, * 3 = 9, - 4 = 5, / 2 = 2, + 1 = 3, * 2 = 6
        let result = run("((((((1 + 2) * 3) - 4) / 2) + 1) * 2);").unwrap();
        assert_eq!(result, Value::Integer(6));
    }

    #[test]
    fn test_modulo_with_negative_dividend() {
        let result = run("-7 % 3;").unwrap();
        assert_eq!(result, Value::Integer(-1)); // Rust semantics: -7 % 3 = -1
    }

    #[test]
    fn test_modulo_with_negative_divisor() {
        let result = run("7 % -3;").unwrap();
        assert_eq!(result, Value::Integer(1)); // Rust semantics: 7 % -3 = 1
    }

    #[test]
    fn test_empty_string_falsy() {
        let result = run(r#"if ("") { 1; } else { 0; }"#).unwrap();
        assert_eq!(result, Value::Integer(0));
    }

    #[test]
    fn test_nonempty_string_truthy() {
        let result = run(r#"if ("x") { 1; } else { 0; }"#).unwrap();
        assert_eq!(result, Value::Integer(1));
    }

    #[test]
    fn test_zero_integer_falsy() {
        let result = run("if (0) { 1; } else { 0; }").unwrap();
        assert_eq!(result, Value::Integer(0));
    }

    #[test]
    fn test_nonzero_integer_truthy() {
        let result = run("if (-1) { 1; } else { 0; }").unwrap();
        assert_eq!(result, Value::Integer(1));
    }

    #[test]
    fn test_empty_array_falsy() {
        let mut interpreter = Interpreter::new_without_env();
        interpreter.set_variable_array("arr", vec![]);
        let result = interpreter.run("if (arr) { 1; } else { 0; }").unwrap();
        assert_eq!(result, Value::Integer(0));
    }

    #[test]
    fn test_nonempty_array_truthy() {
        let mut interpreter = Interpreter::new_without_env();
        interpreter.set_variable_array("arr", vec![Value::Integer(1)]);
        let result = interpreter.run("if (arr) { 1; } else { 0; }").unwrap();
        assert_eq!(result, Value::Integer(1));
    }

    // Method call syntax tests

    #[test]
    fn test_method_call_string_len() {
        let result = run(r#""hello".len();"#).unwrap();
        assert_eq!(result, Value::Integer(5));
    }

    #[test]
    fn test_method_call_string_lowercase() {
        let result = run(r#""HELLO".lowercase();"#).unwrap();
        assert_eq!(result, Value::String("hello".to_string()));
    }

    #[test]
    fn test_method_call_string_uppercase() {
        let result = run(r#""hello".uppercase();"#).unwrap();
        assert_eq!(result, Value::String("HELLO".to_string()));
    }

    #[test]
    fn test_method_call_array_len() {
        let result = run("[1, 2, 3].len();").unwrap();
        assert_eq!(result, Value::Integer(3));
    }

    #[test]
    fn test_method_call_array_push() {
        let result = run("[1, 2].push(3);").unwrap();
        assert_eq!(
            result,
            Value::Array(vec![
                Value::Integer(1),
                Value::Integer(2),
                Value::Integer(3)
            ])
        );
    }

    #[test]
    fn test_method_call_array_get() {
        let result = run(r#"["a", "b", "c"].get(1);"#).unwrap();
        assert_eq!(result, Value::String("b".to_string()));
    }

    #[test]
    fn test_method_call_dict_get() {
        let result = run(r#"{"name": "Alice"}.get("name");"#).unwrap();
        assert_eq!(result, Value::String("Alice".to_string()));
    }

    #[test]
    fn test_method_call_dict_keys() {
        let result = run(r#"{"a": 1, "b": 2}.keys();"#).unwrap();
        assert_eq!(
            result,
            Value::Array(vec![
                Value::String("a".to_string()),
                Value::String("b".to_string())
            ])
        );
    }

    #[test]
    fn test_method_call_chaining() {
        // Test method chaining: trim then lowercase
        let result = run(r#""  HELLO  ".trim().lowercase();"#).unwrap();
        assert_eq!(result, Value::String("hello".to_string()));
    }

    #[test]
    fn test_method_call_chain_array() {
        // Test array method chaining: push twice then get length
        let result = run("[1].push(2).push(3).len();").unwrap();
        assert_eq!(result, Value::Integer(3));
    }

    #[test]
    fn test_method_call_on_variable() {
        let result = run(r#"let s = "HELLO"; s.lowercase();"#).unwrap();
        assert_eq!(result, Value::String("hello".to_string()));
    }

    #[test]
    fn test_method_call_backwards_compatibility() {
        // Both syntaxes should produce the same result
        let func_result = run(r#"len("hello");"#).unwrap();
        let method_result = run(r#""hello".len();"#).unwrap();
        assert_eq!(func_result, method_result);
    }

    #[test]
    fn test_method_call_split() {
        let result = run(r#""a,b,c".split(",");"#).unwrap();
        assert_eq!(
            result,
            Value::Array(vec![
                Value::String("a".to_string()),
                Value::String("b".to_string()),
                Value::String("c".to_string())
            ])
        );
    }

    #[test]
    fn test_method_call_trim_prefix() {
        let result = run(r#""/path/file".trim_prefix("/path");"#).unwrap();
        assert_eq!(result, Value::String("/file".to_string()));
    }

    #[test]
    fn test_method_call_has_prefix() {
        let result = run(r#""hello world".has_prefix("hello");"#).unwrap();
        assert_eq!(result, Value::Boolean(true));
    }

    #[test]
    fn test_method_call_on_grouped_expression() {
        let result = run(r#"("hello" + " world").len();"#).unwrap();
        assert_eq!(result, Value::Integer(11));
    }

    #[test]
    fn test_method_call_undefined_method() {
        let result = run(r#""hello".nonexistent();"#);
        assert!(matches!(
            result,
            Err(crate::FiddlerError::Runtime(
                RuntimeError::UndefinedFunction { .. }
            ))
        ));
    }

    #[test]
    fn test_method_call_with_function_call_args() {
        // Test method with arguments that are function calls
        let result = run(r#""hello".get(len("ab"));"#).unwrap();
        assert_eq!(result, Value::String("l".to_string()));
    }

    #[test]
    fn test_method_call_complex_chain() {
        // Test deeply nested method chains
        let result = run(r#""  HELLO WORLD  ".trim().lowercase().split(" ").len();"#).unwrap();
        assert_eq!(result, Value::Integer(2));
    }

    #[test]
    fn test_method_call_on_function_result() {
        // Call method on result of a function call
        let result = run(r#"str(42).len();"#).unwrap();
        assert_eq!(result, Value::Integer(2));
    }

    #[test]
    fn test_method_and_function_mixed() {
        // Mix method and function syntax in same expression
        let result = run(r#"len("hello".uppercase());"#).unwrap();
        assert_eq!(result, Value::Integer(5));
    }

    // === Contains tests ===

    #[test]
    fn test_array_contains_found() {
        let result = run(r#"[1, 2, 3].contains(2);"#).unwrap();
        assert_eq!(result, Value::Boolean(true));
    }

    #[test]
    fn test_array_contains_not_found() {
        let result = run(r#"[1, 2, 3].contains(5);"#).unwrap();
        assert_eq!(result, Value::Boolean(false));
    }

    #[test]
    fn test_array_contains_string() {
        let result = run(r#"["apple", "banana"].contains("banana");"#).unwrap();
        assert_eq!(result, Value::Boolean(true));
    }

    #[test]
    fn test_dict_contains_key() {
        let result = run(r#"{"name": "Alice", "age": 30}.contains("name");"#).unwrap();
        assert_eq!(result, Value::Boolean(true));
    }

    #[test]
    fn test_dict_contains_key_not_found() {
        let result = run(r#"{"name": "Alice"}.contains("email");"#).unwrap();
        assert_eq!(result, Value::Boolean(false));
    }

    #[test]
    fn test_contains_function_syntax() {
        let result = run(r#"contains([1, 2, 3], 2);"#).unwrap();
        assert_eq!(result, Value::Boolean(true));
    }

    // === Math function tests ===

    #[test]
    fn test_abs_positive() {
        let result = run("abs(42);").unwrap();
        assert_eq!(result, Value::Integer(42));
    }

    #[test]
    fn test_abs_negative() {
        let result = run("abs(-42);").unwrap();
        assert_eq!(result, Value::Integer(42));
    }

    #[test]
    fn test_abs_method_syntax() {
        let result = run("let x = -10; x.abs();").unwrap();
        assert_eq!(result, Value::Integer(10));
    }

    #[test]
    fn test_ceil_identity() {
        let result = run("ceil(42);").unwrap();
        assert_eq!(result, Value::Integer(42));
    }

    #[test]
    fn test_floor_identity() {
        let result = run("floor(42);").unwrap();
        assert_eq!(result, Value::Integer(42));
    }

    #[test]
    fn test_round_identity() {
        let result = run("round(42);").unwrap();
        assert_eq!(result, Value::Integer(42));
    }

    #[test]
    fn test_math_method_syntax() {
        let result = run("42.ceil();").unwrap();
        assert_eq!(result, Value::Integer(42));
    }

    // === Timestamp function tests ===

    #[test]
    fn test_timestamp() {
        let result = run("timestamp();").unwrap();
        if let Value::Integer(ts) = result {
            // Should be after Jan 1, 2020
            assert!(ts > 1577836800);
        } else {
            panic!("Expected integer");
        }
    }

    #[test]
    fn test_epoch_alias() {
        let result = run("epoch();").unwrap();
        if let Value::Integer(ts) = result {
            assert!(ts > 1577836800);
        } else {
            panic!("Expected integer");
        }
    }

    #[test]
    fn test_timestamp_millis() {
        let result = run("timestamp_millis();").unwrap();
        if let Value::Integer(ts) = result {
            // Should be after Jan 1, 2020 in milliseconds
            assert!(ts > 1577836800000);
        } else {
            panic!("Expected integer");
        }
    }

    #[test]
    fn test_timestamp_micros() {
        let result = run("timestamp_micros();").unwrap();
        if let Value::Integer(ts) = result {
            // Should be after Jan 1, 2020 in microseconds
            assert!(ts > 1577836800000000);
        } else {
            panic!("Expected integer");
        }
    }

    #[test]
    fn test_timestamp_iso8601() {
        let result = run("timestamp_iso8601();").unwrap();
        if let Value::String(s) = result {
            // Should be ISO 8601 format
            assert!(s.contains('T'));
            assert!(s.contains('-'));
            assert!(s.contains(':'));
        } else {
            panic!("Expected string");
        }
    }

    #[test]
    fn test_timestamp_in_calculation() {
        // Test that timestamps can be used in arithmetic
        let result =
            run("let start = timestamp_millis(); let end = timestamp_millis(); end - start >= 0;")
                .unwrap();
        assert_eq!(result, Value::Boolean(true));
    }

    // === Float tests ===

    #[test]
    fn test_float_literal() {
        let result = run("3.14;").unwrap();
        assert_eq!(result, Value::Float(3.14));
    }

    #[test]
    fn test_float_arithmetic() {
        let result = run("3.5 + 2.0;").unwrap();
        assert_eq!(result, Value::Float(5.5));
    }

    #[test]
    fn test_float_subtraction() {
        let result = run("10.5 - 3.2;").unwrap();
        if let Value::Float(f) = result {
            assert!((f - 7.3).abs() < 1e-10);
        } else {
            panic!("Expected float");
        }
    }

    #[test]
    fn test_float_multiplication() {
        let result = run("2.5 * 4.0;").unwrap();
        assert_eq!(result, Value::Float(10.0));
    }

    #[test]
    fn test_float_division() {
        let result = run("7.5 / 2.5;").unwrap();
        assert_eq!(result, Value::Float(3.0));
    }

    #[test]
    fn test_mixed_arithmetic_int_float() {
        let result = run("10 + 3.5;").unwrap();
        assert_eq!(result, Value::Float(13.5));
    }

    #[test]
    fn test_mixed_arithmetic_float_int() {
        let result = run("2.5 * 4;").unwrap();
        assert_eq!(result, Value::Float(10.0));
    }

    #[test]
    fn test_float_comparison() {
        let result = run("3.14 > 3.0;").unwrap();
        assert_eq!(result, Value::Boolean(true));
    }

    #[test]
    fn test_float_equality() {
        let result = run("2.5 == 2.5;").unwrap();
        assert_eq!(result, Value::Boolean(true));
    }

    #[test]
    fn test_cross_type_equality() {
        let result = run("1.0 == 1;").unwrap();
        assert_eq!(result, Value::Boolean(true));
    }

    #[test]
    fn test_float_negation() {
        let result = run("-3.14;").unwrap();
        assert_eq!(result, Value::Float(-3.14));
    }

    #[test]
    fn test_float_truthy() {
        let result = run("if (1.5) { 1; } else { 0; }").unwrap();
        assert_eq!(result, Value::Integer(1));
    }

    #[test]
    fn test_float_zero_falsy() {
        let result = run("if (0.0) { 1; } else { 0; }").unwrap();
        assert_eq!(result, Value::Integer(0));
    }

    #[test]
    fn test_float_conversion() {
        let result = run("float(42);").unwrap();
        assert_eq!(result, Value::Float(42.0));
    }

    #[test]
    fn test_float_conversion_from_string() {
        let result = run(r#"float("3.14");"#).unwrap();
        assert_eq!(result, Value::Float(3.14));
    }

    #[test]
    fn test_int_from_float() {
        let result = run("int(3.99);").unwrap();
        assert_eq!(result, Value::Integer(3));
    }

    #[test]
    fn test_ceil_float() {
        let result = run("ceil(3.14);").unwrap();
        assert_eq!(result, Value::Integer(4));
    }

    #[test]
    fn test_floor_float() {
        let result = run("floor(3.99);").unwrap();
        assert_eq!(result, Value::Integer(3));
    }

    #[test]
    fn test_round_float() {
        let result = run("round(3.5);").unwrap();
        assert_eq!(result, Value::Integer(4));
    }

    #[test]
    fn test_abs_float() {
        let result = run("abs(-3.14);").unwrap();
        assert_eq!(result, Value::Float(3.14));
    }

    #[test]
    fn test_float_method_syntax() {
        let result = run("let x = -2.5; x.abs();").unwrap();
        assert_eq!(result, Value::Float(2.5));
    }

    #[test]
    fn test_float_in_variable() {
        let result = run("let pi = 3.14159; pi * 2.0;").unwrap();
        if let Value::Float(f) = result {
            assert!((f - 6.28318).abs() < 1e-5);
        } else {
            panic!("Expected float");
        }
    }

    #[test]
    fn test_float_division_by_zero() {
        let result = run("1.0 / 0.0;").unwrap();
        if let Value::Float(f) = result {
            assert!(f.is_infinite() && f.is_sign_positive());
        } else {
            panic!("Expected float infinity");
        }
    }

    #[test]
    fn test_nan_creation() {
        let result = run("0.0 / 0.0;").unwrap();
        if let Value::Float(f) = result {
            assert!(f.is_nan());
        } else {
            panic!("Expected NaN");
        }
    }

    #[test]
    fn test_mixed_comparison() {
        let result = run("1 < 1.5;").unwrap();
        assert_eq!(result, Value::Boolean(true));
    }
}
