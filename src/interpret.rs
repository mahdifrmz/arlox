use crate::ast::{
    Ast, BinaryExpr, ExprStmt, GroupExpr, LiteralExpr, NodeVisitor, PrintStmt, Program, UnaryExpr,
    VarDecl,
};

use crate::environment::{Env, Environment};
use crate::{
    ast::{AssignExpr, Block, BreakStmt, FunCall, FunDecl, FunDef, IfStmt, ReturnStmt, WhileStmt},
    function::{all_natives, Function, Implementation},
    lox_error,
    token::{Token, TokenKind},
};
use std::fmt::Display;

#[derive(PartialEq, Clone)]
pub enum Value {
    Number(f64),
    String(String),
    Boolean(bool),
    Function(Function),
    Nil,
}

struct Interpretor {
    env: Env,
    breaking: Option<Token>,
    returning: Option<(Token, Value)>,
}

pub fn interpret(ast: Ast) -> Option<Value> {
    let mut interpretor = Interpretor::new();
    for (name, nf) in all_natives() {
        interpretor.env.borrow_mut().init(name, Value::Function(nf));
    }

    let rsl = ast.root().visit(&mut interpretor).ok();
    if let Some(tkn) = interpretor.breaking {
        lox_error(tkn.line(), "break statement out of loop");
        None
    } else if let Some((tkn, _)) = interpretor.returning {
        lox_error(tkn.line(), "return statement out of function");
        None
    } else {
        rsl
    }
}

fn check_arity(params: &Vec<String>, arg_count: usize) -> Option<usize> {
    let pcount;
    let err;

    if params.last() == Some(&"".to_string()) {
        pcount = params.len() - 1;
        err = arg_count < pcount;
    } else {
        pcount = params.len();
        err = arg_count != pcount;
    }
    if err {
        return Some(pcount);
    }
    None
}

impl Interpretor {
    fn new() -> Interpretor {
        Interpretor {
            env: Environment::new(None),
            breaking: None,
            returning: None,
        }
    }
    fn _env_global(env: Env) -> Env {
        match env.borrow().parent().as_ref() {
            None => env.clone(),
            Some(p) => Self::_env_global(p.clone()),
        }
    }
    fn env_global(&self) -> Env {
        Self::_env_global(self.env.clone())
    }
    fn visit_plus(&mut self, node: &BinaryExpr) -> Result<Value, ()> {
        match (node.lexpr().visit(self)?, node.rexpr().visit(self)?) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a + b)),
            (Value::String(a), Value::String(b)) => Ok(Value::String(format!("{}{}", a, b))),
            _ => {
                crate::lox_error(
                    node.token().line(),
                    "operator '+' can only be used on 2 numbers or 2 strings",
                );
                Err(())
            }
        }
    }
    fn visit_math(&mut self, node: &BinaryExpr) -> Result<Value, ()> {
        match (node.lexpr().visit(self)?, node.rexpr().visit(self)?) {
            (Value::Number(a), Value::Number(b)) => match node.token().kind() {
                TokenKind::Star => Ok(Value::Number(a * b)),
                TokenKind::Slash => Ok(Value::Number(a / b)),
                TokenKind::Minus => Ok(Value::Number(a - b)),
                TokenKind::GreaterEqual => Ok(Value::Boolean(a >= b)),
                TokenKind::LessEqual => Ok(Value::Boolean(a <= b)),
                TokenKind::Less => Ok(Value::Boolean(a < b)),
                TokenKind::Greater => Ok(Value::Boolean(a > b)),
                _ => Err(()),
            },
            _ => {
                crate::lox_error(
                    node.token().line(),
                    "arithmatic operators can only be used on numbers",
                );
                Err(())
            }
        }
    }
    fn visit_and(&mut self, node: &BinaryExpr) -> Result<Value, ()> {
        let left = node.lexpr().visit(self)?;
        if !left.truth() {
            return Ok(Value::Boolean(false));
        }
        node.rexpr().visit(self)
    }
    fn visit_or(&mut self, node: &BinaryExpr) -> Result<Value, ()> {
        let left = node.lexpr().visit(self)?;
        if left.truth() {
            return Ok(Value::Boolean(true));
        }
        node.rexpr().visit(self)
    }
}

impl NodeVisitor for Interpretor {
    type Retval = Result<Value, ()>;
    fn visit_literal(&mut self, node: &LiteralExpr) -> Result<Value, ()> {
        match node.token().kind() {
            TokenKind::Nil => Ok(Value::Nil),
            TokenKind::Number => match node.token().text().parse::<f64>() {
                Ok(num) => Ok(Value::Number(num)),
                Err(_) => {
                    crate::lox_error(
                        node.token().line(),
                        &format!("invalid number ({})", node.token().text()),
                    );
                    Err(())
                }
            },
            TokenKind::String => Ok(Value::String(
                node.token().text()[1..node.token().text().len() - 1].to_owned(),
            )),
            TokenKind::True => Ok(Value::Boolean(true)),
            TokenKind::False => Ok(Value::Boolean(false)),
            TokenKind::Identifier => match self.env.borrow_mut().get(node.token().text()) {
                Some(v) => Ok(v.clone()),
                None => {
                    lox_error(
                        node.token().line(),
                        format!("undefind variable '{}'", node.token().text()).as_str(),
                    );
                    Err(())
                }
            },
            _ => Err(()),
        }
    }
    fn visit_group(&mut self, node: &GroupExpr) -> Result<Value, ()> {
        node.expr().visit(self)
    }
    fn visit_assignment(&mut self, node: &AssignExpr) -> Result<Value, ()> {
        let value = node.expr().visit(self)?;
        self.env
            .borrow_mut()
            .assign(node.variable().text().clone(), value.clone());
        Ok(value)
    }
    fn visit_if_stmt(&mut self, node: &IfStmt) -> Result<Value, ()> {
        let condition = node.expr().visit(self)?;
        if condition.truth() {
            node.stmt().visit(self)?;
        } else {
            if let Some(elstmt) = node.elstmt() {
                elstmt.visit(self)?;
            }
        }
        Ok(Value::Nil)
    }
    fn visit_while_stmt(&mut self, node: &WhileStmt) -> Result<Value, ()> {
        while node.expr().visit(self)?.truth() {
            node.stmt().visit(self)?;
            if let Some(_) = self.breaking {
                self.breaking = None;
                break;
            }
        }
        Ok(Value::Nil)
    }
    fn visit_break_stmt(&mut self, node: &BreakStmt) -> Result<Value, ()> {
        self.breaking = Some(node.token().clone());
        Ok(Value::Nil)
    }
    fn visit_return_stmt(&mut self, node: &ReturnStmt) -> Result<Value, ()> {
        let value = match node.expr() {
            Some(e) => e.visit(self)?,
            None => Value::Nil,
        };
        self.returning = Some((node.token().clone(), value));
        Ok(Value::Nil)
    }
    fn visit_unary(&mut self, node: &UnaryExpr) -> Result<Value, ()> {
        if node.token().kind() == TokenKind::Bang {
            Ok(Value::Boolean(!node.expr().visit(self)?.truth()))
        } else {
            match node.expr().visit(self)? {
                Value::Number(num) => Ok(Value::Number(-num)),
                _ => {
                    crate::lox_error(node.token().line(), "expected number after '-'");
                    Err(())
                }
            }
        }
    }
    fn visit_binary(&mut self, node: &BinaryExpr) -> Result<Value, ()> {
        match node.token().kind() {
            TokenKind::EqualEqual => Ok(Value::Boolean(
                node.lexpr().visit(self) == node.rexpr().visit(self),
            )),
            TokenKind::BangEqual => Ok(Value::Boolean(
                node.lexpr().visit(self) != node.rexpr().visit(self),
            )),
            TokenKind::Or => self.visit_or(node),
            TokenKind::And => self.visit_and(node),
            TokenKind::Plus => self.visit_plus(node),
            _ => self.visit_math(node),
        }
    }
    fn visit_print_stmt(&mut self, node: &PrintStmt) -> Result<Value, ()> {
        let value = node.expr().visit(self)?;
        println!("{}", value);
        Ok(Value::Nil)
    }
    fn visit_expr_stmt(&mut self, node: &ExprStmt) -> Result<Value, ()> {
        node.expr().visit(self)?;
        Ok(Value::Nil)
    }
    fn visit_var_decl(&mut self, node: &VarDecl) -> Result<Value, ()> {
        let value = match node.expr() {
            Some(e) => e.visit(self)?,
            None => Value::Nil,
        };
        self.env
            .borrow_mut()
            .init(node.name().text().clone(), value);

        Ok(Value::Nil)
    }
    fn visit_fun_decl(&mut self, node: &FunDecl) -> Result<Value, ()> {
        let name = node.name().text().clone();
        self.env.borrow_mut().init(
            name.clone(),
            Value::Function(Function::create(
                Implementation::LoxImpl(node.block().clone()),
                node.params().iter().map(|t| t.text().clone()).collect(),
                Some(self.env.clone()),
            )),
        );
        Ok(Value::Nil)
    }
    fn visit_fun_def(&mut self, node: &FunDef) -> Result<Value, ()> {
        Ok(Value::Function(Function::create(
            Implementation::LoxImpl(node.block().clone()),
            node.params().iter().map(|t| t.text().clone()).collect(),
            Some(self.env.clone()),
        )))
    }
    fn visit_fun_call(&mut self, node: &FunCall) -> Result<Value, ()> {
        let line = node.line();
        let callee = node.callee().visit(self)?;
        let callee = match callee {
            Value::Function(fun) => fun,
            _ => {
                lox_error(line, format!("{} is not callable", callee).as_str());
                return Err(());
            }
        };
        if let Some(pcount) = check_arity(callee.params(), node.args().len()) {
            lox_error(
                line,
                format!(
                    "invalid number of arguments ({}) passed to function which takes {} params",
                    node.args().len(),
                    pcount,
                )
                .as_str(),
            );
            return Err(());
        }
        let new_env = callee.closure().unwrap_or_else(|| self.env_global());
        let new_env = Environment::new(Some(new_env));
        let mut args = vec![];
        for a in node.args() {
            args.push(a.visit(self)?);
        }
        match callee.code() {
            Implementation::NativeImpl(nf) => Ok(nf(args)?),
            Implementation::LoxImpl(lf) => {
                let prev = self.env.clone();
                self.env = new_env;
                for (p, a) in callee.params().iter().zip(args.iter()) {
                    self.env.borrow_mut().init(p.clone(), a.clone())
                }
                lf.visit(self)?;
                self.env = prev;

                if let Some((_, value)) = self.returning.clone() {
                    self.returning = None;
                    Ok(value)
                } else {
                    Ok(Value::Nil)
                }
            }
        }
    }

    fn visit_program(&mut self, node: &Program) -> Result<Value, ()> {
        for s in node.decs() {
            s.visit(self)?;
            if let Some(_) = self.breaking {
                break;
            }
            if let Some(_) = self.returning {
                break;
            }
        }
        Ok(Value::Nil)
    }
    fn visit_block(&mut self, node: &Block) -> Result<Value, ()> {
        let parent = self.env.clone();
        let branch = Environment::new(Some(parent.clone()));
        self.env = branch;
        for s in node.decs() {
            s.visit(self)?;
            if let Some(_) = self.breaking {
                break;
            }
            if let Some(_) = self.returning {
                break;
            }
        }
        self.env = parent;
        Ok(Value::Nil)
    }
}

impl Value {
    fn truth(&self) -> bool {
        if *self == Value::Nil || *self == Value::Boolean(false) {
            false
        } else {
            true
        }
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let rep = match self {
            Value::Number(num) => num.to_string(),
            Value::String(s) => s.clone(),
            Value::Nil => String::from("NIL"),
            Value::Boolean(b) => b.to_string(),
            Value::Function(f) => match f.code() {
                Implementation::NativeImpl(_) => "[Native Function]".to_string(),
                Implementation::LoxImpl(_) => "[Function]".to_string(),
            },
        };
        write!(f, "{}", rep)
    }
}
