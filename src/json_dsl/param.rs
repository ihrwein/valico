use rustc_serialize::json::{self, ToJson};
use regex;
use url;

use super::super::json_schema;
use super::builder;
use super::coercers;
use super::validators;

pub struct Param {
    pub name: String,
    pub coercer: Option<Box<coercers::Coercer + Send + Sync>>,
    pub nest: Option<builder::Builder>,
    pub description: Option<String>,
    pub allow_null: bool,
    pub validators: validators::Validators,
    pub default: Option<json::Json>,
    pub schema_builder: Option<Box<Fn(&mut json_schema::Builder) + Send + Sync>>,
    pub schema_id: Option<url::Url>
}

unsafe impl Send for Param { }

impl Param {

    pub fn new(name: &str) -> Param {
        Param {
            name: name.to_string(),
            description: None,
            coercer: None,
            nest: None,
            allow_null: false,
            validators: vec![],
            default: None,
            schema_builder: None,
            schema_id: None
        }
    }

    pub fn new_with_coercer(name: &str, coercer: Box<coercers::Coercer  + Send + Sync>) -> Param {
        Param {
            name: name.to_string(),
            description: None,
            coercer: Some(coercer),
            nest: None,
            allow_null: false,
            validators: vec![],
            default: None,
            schema_builder: None,
            schema_id: None
        }
    }

    pub fn new_with_nest(name: &str, coercer: Box<coercers::Coercer + Send + Sync>, nest: builder::Builder) -> Param {
        Param {
            name: name.to_string(),
            description: None,
            coercer: Some(coercer),
            nest: Some(nest),
            allow_null: false,
            validators: vec![],
            default: None,
            schema_builder: None,
            schema_id: None
        }
    }

    pub fn build<F>(name: &str, build_def: F) -> Param where F: FnOnce(&mut Param) {
        let mut param = Param::new(name);
        build_def(&mut param);

        param
    }

    pub fn desc(&mut self, description: &str) {
        self.description = Some(description.to_string());
    }

    pub fn schema<F>(&mut self, build: F) where F: Fn(&mut json_schema::Builder,) + Send + Sync {
        self.schema_builder = Some(Box::new(build));
    }

    pub fn coerce(&mut self, coercer: Box<coercers::Coercer + Send + Sync>) {
        self.coercer = Some(coercer);
    }

    pub fn nest<F>(&mut self, nest_def: F) where F: FnOnce(&mut builder::Builder) -> () {
        self.nest = Some(builder::Builder::build(nest_def));
    }

    pub fn allow_null(&mut self) {
        self.allow_null = true;
    }

    pub fn regex(&mut self, regex: regex::Regex) {
        self.validators.push(Box::new(regex));
    }

    pub fn validate(&mut self, validator: Box<validators::Validator + 'static + Send + Sync>) {
        self.validators.push(validator);
    }

    pub fn validate_with<F>(&mut self, validator: F) where F: Fn(&json::Json, &str, bool) -> super::validators::ValidatorResult + Send+Sync {
        self.validators.push(Box::new(validator));
    }

    fn process_validators(&self, val: &json::Json, path: &str) -> super::super::ValicoErrors {
        let mut errors = vec![];
        for validator in self.validators.iter() {
            match validator.validate(val, path, true) {
                Ok(()) => (),
                Err(mut validation_errors) => errors.append(&mut validation_errors)
            }
        };

        errors
    }

    pub fn process(&self, val: &mut json::Json, path: &str, scope: &Option<&json_schema::Scope>) -> super::ExtendedResult<Option<json::Json>> {
        if val.is_null() && self.allow_null { 
            return super::ExtendedResult::new(None) 
        }

        let mut result = super::ExtendedResult::new(None);
        let mut return_value = None;

        {

            let val = if self.coercer.is_some() {
                match self.coercer.as_ref().unwrap().coerce(val, path) {
                    Ok(None) => val,
                    Ok(Some(new_value)) => {
                        return_value = Some(new_value); 
                        return_value.as_mut().unwrap() 
                    },
                    Err(mut errors) => {
                        result.state.errors.append(&mut errors);
                        return result;
                    }
                }
            } else {
                val
            };

            if self.nest.is_some() {
                let mut process_state = self.nest.as_ref().unwrap().process_nest(val, path, scope);
                result.append(&mut process_state);
            }

            let mut validation_errors = self.process_validators(val, path);
            result.state.errors.append(&mut validation_errors);

            if self.schema_id.is_some() && scope.is_some() {
                let id = self.schema_id.as_ref().unwrap();
                let schema = scope.as_ref().unwrap().resolve(id);
                match schema {
                    Some(schema) => result.append(&mut schema.validate(&val)),
                    None => result.state.missing.push(id.clone())
                }
            }
        }

        if return_value.is_some() {
            result.value = return_value;
        }
        
        result
    }
}

impl Param {
    pub fn allow_values<T: json::ToJson>(&mut self, values: &[T]) {
        self.validators.push(Box::new(validators::AllowedValues::new(
            values.iter().map(|v| v.to_json()).collect()
        )));
    }

    pub fn reject_values<T: json::ToJson>(&mut self, values: &[T]) {
        self.validators.push(Box::new(validators::RejectedValues::new(
            values.iter().map(|v| v.to_json()).collect()
        )));
    }

    pub fn default<T: json::ToJson>(&mut self, default: T) {
        self.default = Some(default.to_json());
    }
}
