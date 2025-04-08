//! Umazen Base Calculator - High-Precision Arithmetic Module

#![forbid(unsafe_code)]
#![warn(
    missing_docs,
    trivial_casts,
    trivial_numeric_casts,
    unused_import_braces,
    unused_qualifications
)]

use anchor_lang::prelude::*;
use num_traits::{CheckedAdd, CheckedSub, CheckedMul, CheckedDiv, Signed};
use fixed::types::I80F48;
use solana_program::program_error::ProgramError;
use std::convert::TryInto;
use thiserror::Error;

/// Mathematical operation context
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, Default)]
pub struct CalculationContext {
    pub precision: u32,
    pub rounding_mode: RoundingMode,
    pub overflow_protection: bool,
    pub max_iterations: u32,
    pub enable_parallel: bool,
}

/// Supported rounding modes
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq)]
pub enum RoundingMode {
    Nearest,
    Zero,
    Up,
    Down,
}

impl Default for RoundingMode {
    fn default() -> Self {
        RoundingMode::Nearest
    }
}

/// Mathematical operation parameters
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub enum Operation {
    Add {
        a: I80F48,
        b: I80F48,
    },
    Sub {
        a: I80F48,
        b: I80F48,
    },
    Mul {
        a: I80F48,
        b: I80F48,
    },
    Div {
        dividend: I80F48,
        divisor: I80F48,
    },
    Pow {
        base: I80F48,
        exponent: i32,
    },
    Sqrt {
        value: I80F48,
    },
    Log {
        value: I80F48,
        base: I80F48,
    },
}

/// Calculation result with verification data
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct CalculationResult {
    pub value: I80F48,
    pub proof: [u8; 32],
    pub computation_units: u64,
    pub timestamp: i64,
}

#[program]
pub mod base_calculator {
    use super::*;

    /// Execute mathematical operation with verification
    pub fn calculate(ctx: Context<Calculate>, 
                    op: Operation,
                    config: CalculationContext) -> Result<CalculationResult> {
        // Validate input parameters
        validate_operation(&op, &config)?;

        // Execute calculation
        let result = match op {
            Operation::Add { a, b } => execute_add(a, b, &config),
            Operation::Sub { a, b } => execute_sub(a, b, &config),
            Operation::Mul { a, b } => execute_mul(a, b, &config),
            Operation::Div { dividend, divisor } => execute_div(dividend, divisor, &config),
            Operation::Pow { base, exponent } => execute_pow(base, exponent, &config),
            Operation::Sqrt { value } => execute_sqrt(value, &config),
            Operation::Log { value, base } => execute_log(value, base, &config),
        }?;

        // Generate verification proof
        let proof = generate_proof(&op, result)?;

        Ok(CalculationResult {
            value: result,
            proof,
            computation_units: calculate_computation_units(&op),
            timestamp: Clock::get()?.unix_timestamp,
        })
    }
}

/// Core arithmetic implementations
impl BaseCalculator {
    fn execute_add(a: I80F48, b: I80F48, config: &CalculationContext) -> Result<I80F48> {
        apply_overflow_protection(a.checked_add(b), config)
    }

    fn execute_sub(a: I80F48, b: I80F48, config: &CalculationContext) -> Result<I80F48> {
        apply_overflow_protection(a.checked_sub(b), config)
    }

    fn execute_mul(a: I80F48, b: I80F48, config: &CalculationContext) -> Result<I80F48> {
        apply_overflow_protection(a.checked_mul(b), config)
    }

    fn execute_div(dividend: I80F48, divisor: I80F48, config: &CalculationContext) -> Result<I80F48> {
        require!(!divisor.is_zero(), CalculatorError::DivisionByZero);
        apply_overflow_protection(dividend.checked_div(divisor), config)
    }

    fn execute_pow(base: I80F48, exponent: i32, config: &CalculationContext) -> Result<I80F48> {
        let mut result = I80F48::from_num(1);
        let abs_exponent = exponent.abs() as u32;
        let mut current_exponent = 0u32;

        while current_exponent < abs_exponent {
            result = apply_overflow_protection(result.checked_mul(base), config)?;
            current_exponent += 1;
            
            if current_exponent % config.max_iterations == 0 {
                check_resource_limits(current_exponent, config.max_iterations)?;
            }
        }

        if exponent < 0 {
            apply_overflow_protection(I80F48::from_num(1).checked_div(result), config)
        } else {
            Ok(result)
        }
    }

    fn execute_sqrt(value: I80F48, config: &CalculationContext) -> Result<I80F48> {
        require!(value >= I80F48::ZERO, CalculatorError::NegativeRoot);
        let precision = config.precision;
        let mut guess = value / I80F48::from_num(2);
        let mut iteration = 0;

        loop {
            let new_guess = (guess + value / guess) / I80F48::from_num(2);
            let delta = (new_guess - guess).abs();

            guess = new_guess;
            iteration += 1;

            if delta <= I80F48::epsilon(precision) || iteration >= config.max_iterations {
                break;
            }

            if iteration % config.max_iterations == 0 {
                check_resource_limits(iteration, config.max_iterations)?;
            }
        }

        apply_rounding(guess, config)
    }

    fn execute_log(value: I80F48, base: I80F48, config: &CalculationContext) -> Result<I80F48> {
        require!(value > I80F48::ZERO, CalculatorError::LogNonPositive);
        require!(base > I80F48::ZERO && base != I80F48::ONE, CalculatorError::InvalidLogBase);

        let ln_val = calculate_ln(value, config)?;
        let ln_base = calculate_ln(base, config)?;

        apply_overflow_protection(ln_val.checked_div(ln_base), config)
    }
}

/// Helper functions
fn apply_overflow_protection(result: Option<I80F48>, config: &CalculationContext) -> Result<I80F48> {
    result.ok_or_else(|| {
        if config.overflow_protection {
            CalculatorError::ArithmeticOverflow.into()
        } else {
            msg!("Overflow occurred but protection is disabled");
            CalculatorError::UnsafeOperation.into()
        }
    })
}

fn apply_rounding(value: I80F48, config: &CalculationContext) -> I80F48 {
    match config.rounding_mode {
        RoundingMode::Nearest => value.round_to_precision(config.precision),
        RoundingMode::Zero => value.trunc_to_precision(config.precision),
        RoundingMode::Up => value.ceil_to_precision(config.precision),
        RoundingMode::Down => value.floor_to_precision(config.precision),
    }
}

fn generate_proof(op: &Operation, result: I80F48) -> Result<[u8; 32]> {
    let mut hasher = sha3::Sha3_256::new();
    
    match op {
        Operation::Add { a, b } => {
            hasher.update(a.to_be_bytes());
            hasher.update(b.to_be_bytes());
        },
        // Other operation variants...
    }
    
    hasher.update(result.to_be_bytes());
    Ok(hasher.finalize().into())
}

/// Error handling
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum CalculatorError {
    #[error("Arithmetic overflow detected")]
    ArithmeticOverflow,
    #[error("Division by zero attempted")]
    DivisionByZero,
    #[error("Invalid input parameters")]
    InvalidInput,
    #[error("Maximum iterations exceeded")]
    MaxIterationsExceeded,
    #[error("Negative value for root operation")]
    NegativeRoot,
    #[error("Logarithm of non-positive number")]
    LogNonPositive,
    #[error("Invalid logarithm base")]
    InvalidLogBase,
    #[error("Unsafe operation attempted")]
    UnsafeOperation,
}

impl From<CalculatorError> for ProgramError {
    fn from(e: CalculatorError) -> Self {
        ProgramError::Custom(e as u32 + 1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fixed::macros::*;

    #[test]
    fn test_safe_addition() {
        let config = CalculationContext {
            overflow_protection: true,
            ..Default::default()
        };
        
        let result = BaseCalculator::execute_add(
            fixed!(340282366920938463463.374607431768211455: I80F48),
            fixed!(1: I80F48),
            &config
        );
        
        assert_eq!(result, Err(CalculatorError::ArithmeticOverflow.into()));
    }

    #[test]
    fn test_high_precision_division() {
        let config = CalculationContext {
            precision: 12,
            rounding_mode: RoundingMode::Nearest,
            ..Default::default()
        };
        
        let result = BaseCalculator::execute_div(
            fixed!(1: I80F48),
            fixed!(3: I80F48),
            &config
        ).unwrap();
        
        assert_eq!(result.to_string(), "0.333333333333");
    }
}
