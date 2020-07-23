
use crate::{model::{Value, Env, RuntimeError, Lambda}, utils::vec_to_cons};
use std::{collections::HashMap, rc::Rc, cell::{RefCell}};

// Treat the given expression as a cons list of expressions (a function body, 
// for example). Each expression is evaluated in order and the final one's 
// retur value is returned.
fn evaluate_block(env: Rc<RefCell<Env>>, body: &Value) -> Result<Value,RuntimeError> {
  let mut result = None;

  for line in body.as_list().unwrap().into_iter() {
    result = Some(eval(env.clone(), &line));
  }

  return result.unwrap_or(Ok(Value::Nil));
}

/// Evaluate a given Lisp expression in the context of a given environment.
pub fn eval(env: Rc<RefCell<Env>>, expression: &Value) -> Result<Value,RuntimeError> {
  
  let result: Result<Value,RuntimeError> = match expression {

    // look up symbol
    Value::Symbol(symbol) => match env.borrow().find(&symbol) {
      Some(expr) => Ok(expr.clone()),
      None => Err(RuntimeError { msg: format!("\"{}\" is not defined", symbol) }),
    },

    // s-expression
    Value::List(list) => {
      match &list.car {

        // special forms
        Value::Symbol(symbol) if symbol == "define" => {
          let cdr = list.cdr.clone().unwrap();
          let symbol = cdr.car.as_symbol().unwrap();
          let value_expr = &cdr.cdr.clone().unwrap().car;
          let value = eval(env.clone(), value_expr)?;

          env.borrow_mut().entries.insert(symbol, value.clone());

          Ok(value)
        },

        Value::Symbol(symbol) if symbol == "set" => {
          let cdr = list.cdr.clone().unwrap();
          let symbol = cdr.car.as_symbol().unwrap();
          let value_expr = &cdr.cdr.clone().unwrap().car;
          let value = eval(env.clone(), value_expr)?;

          if env.borrow().entries.contains_key(&symbol) {
            env.borrow_mut().entries.insert(symbol, value.clone());
          } else {
            let mut focal_env: Option<Rc<RefCell<Env>>> = env.borrow().parent.clone();
  
            while focal_env.as_ref().map_or(false, |e| !e.borrow().entries.contains_key(&symbol)) {
              let rc = focal_env.unwrap();
              focal_env = rc.borrow().parent.clone();
            }

            if let Some(env) = focal_env {
              env.borrow_mut().entries.insert(symbol, value.clone());
            } else {
              return Err(RuntimeError { msg: format!("Tried to set value of undefined symbol \"{}\"", symbol) });
            }
          }

          Ok(value)
        },

        Value::Symbol(symbol) if symbol == "defun" => {
          let mut list_iter = list.into_iter();
          list_iter.next().unwrap(); // skip "defun"
          let symbol = list_iter.next().unwrap().as_symbol().unwrap();
          let argnames = Rc::new(list_iter.next().unwrap().clone());
          let body = Rc::new(vec_to_cons(&list_iter.map(|v| v.clone()).collect()));

          let lambda = Value::Lambda(Lambda {
            closure: env.clone(),
            argnames,
            body
          });

          env.borrow_mut().entries.insert(symbol, lambda);

          Ok(Value::Nil)
        },

        Value::Symbol(symbol) if symbol == "lambda" => {
          let cdr = list.cdr.as_ref().unwrap();
          let argnames = Rc::new(cdr.car.clone());
          let body = Rc::new(Value::List(cdr.cdr.clone().unwrap()));

          Ok(Value::Lambda(Lambda {
            closure: env.clone(),
            argnames,
            body
          }))
        },

        Value::Symbol(symbol) if symbol == "quote" => {
          let exp = list.cdr.as_ref().unwrap().car.clone();

          Ok(exp)
        },

        Value::Symbol(symbol) if symbol == "let" => {
          let let_env = Rc::new(RefCell::new(Env {
            parent: Some(env.clone()),
            entries: HashMap::new()
          }));
          let declarations = list.cdr.as_ref().map(|c| &c.car).unwrap();

          for decl in declarations.as_list().unwrap().into_iter() {
            let decl_cons = decl.as_list().unwrap();
            let symbol = decl_cons.car.as_symbol().unwrap();
            let expr = &decl_cons.cdr.as_ref().unwrap().car;

            let result = eval(let_env.clone(), &expr)?;
            let_env.borrow_mut().entries.insert(symbol, result);
          }

          let body = Value::List(list.cdr.as_ref().unwrap().cdr.clone().unwrap());

          evaluate_block(let_env.clone(), &body)
        },

        Value::Symbol(symbol) if symbol == "begin" => {
          let body = Value::List(list.cdr.clone().unwrap());

          evaluate_block(env.clone(), &body)
        },

        Value::Symbol(symbol) if symbol == "cond" => {
          let clauses = list.cdr.as_ref().unwrap();
          let mut result = Value::Nil;

          for clause in clauses.into_iter().map(|clause| clause.as_list().unwrap()) {
            let condition = &clause.car;
            let then = &clause.cdr.as_ref().unwrap().car;

            if eval(env.clone(), condition)?.is_truthy() {
              result = eval(env.clone(), then)?;
              break;
            }
          }

          Ok(result)
        },

        Value::Symbol(symbol) if symbol == "if" => {
          let cdr = list.cdr.as_ref().unwrap();
          let condition = &cdr.as_ref().car;
          let then_result = &cdr.as_ref().cdr.as_ref().unwrap().car;
          let else_result = cdr.as_ref().cdr.as_ref().unwrap().cdr.as_ref().map(|c| &c.car);

          if eval(env.clone(), condition)?.is_truthy() {
            Ok(eval(env.clone(), then_result)?)
          } else {
            Ok(match else_result {
              Some(v) => eval(env.clone(), v)?,
              None => Value::Nil
            })
          }
        },

        Value::Symbol(symbol) if symbol == "and" => {
          let cdr = list.cdr.as_ref().unwrap();
          let a = &cdr.car;
          let b = &cdr.cdr.as_ref().unwrap().car;

          Ok(Value::from_truth(
              eval(env.clone(), a)?.is_truthy() 
              && eval(env.clone(), b)?.is_truthy()
          ))
        },

        Value::Symbol(symbol) if symbol == "or" => {
          let cdr = list.cdr.as_ref().unwrap();
          let a = &cdr.car;
          let b = &cdr.cdr.as_ref().unwrap().car;

          Ok(Value::from_truth(
              eval(env.clone(), a)?.is_truthy() 
              || eval(env.clone(), b)?.is_truthy()
          ))
        },

        Value::TailCall(v) => Ok(eval(env.clone(), &v)?),

        // function call
        _ => {
          let func = eval(env.clone(), &list.car)?;
          let args = list.into_iter().skip(1)
            .map(|car| eval(env.clone(), car).map_err(|e| e.clone()));

          // if within_function_call && is_return_value && !tail_position_found {
          //   let expr = Value::TailCall(Rc::new(Value::List(Rc::new(ConsCell {
          //     car: func,
          //     cdr: Some(vec_to_cons(&args.collect()).as_list().unwrap())
          //   }))));
          //   println!("tail-calling: {}", &expr);
          //   Ok(expr)
          // } else {

          match func {

            // call native function
            Value::NativeFunc(func) => {
              let args_vec: Vec<Result<Value,RuntimeError>> = args.collect();

              let err = args_vec.iter()
                .find_map(|a| a.clone().err());
              
              let args_vec = args_vec.iter()
                .filter_map(|a| a.clone().ok())
                .collect();

              match err {
                Some(e) => Err(e),
                None => func(env.clone(), &args_vec)
              }
            },

            // call lambda function
            Value::Lambda(lamb) => {
              let argnames = lamb.argnames.as_list().unwrap();
    
              // bind args
              let mut entries: HashMap<String,Value> = HashMap::new();
              
              for (arg_name, arg_value) in argnames.into_iter().zip(args) {
                let name = arg_name.as_symbol().unwrap();
                entries.insert(name, arg_value?.clone());
              }
    
              let arg_env = Rc::new(RefCell::new(Env {
                parent: Some(env.clone()),
                entries
              }));
                  
              // evaluate each line of body
              evaluate_block(arg_env.clone(), &lamb.body)
            }
            _ => Err(RuntimeError { msg: String::from("Argument 0 is not callable") })
          }
        }
      }
    },

    // plain value
    _ => Ok(expression.clone()),
  };

  if let Ok(Value::TailCall(expr)) = &result {
    return eval(env.clone(), expr.as_ref());
  } else {
    return result;
  }
}
