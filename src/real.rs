// SPDX-License-Identifier: MIT OR Apache-2.0

pub trait Real:
    Copy
    + PartialOrd
    + core::ops::Add<Output = Self>
    + core::ops::Sub<Output = Self>
    + core::ops::Mul<Output = Self>
    + core::ops::Div<Output = Self>
{
    fn zero() -> Self;
    fn one() -> Self;
    fn from_f64(value: f64) -> Self;
    fn from_usize(value: usize) -> Self;

    fn sqrt(self) -> Self;
    fn ln(self) -> Self;
    fn abs(self) -> Self;
    fn powi(self, n: i32) -> Self;
    fn max(self, other: Self) -> Self;
    fn min(self, other: Self) -> Self;
    fn is_finite(self) -> bool;
}

impl Real for f64 {
    fn zero() -> Self {
        0.0
    }

    fn one() -> Self {
        1.0
    }

    fn from_f64(value: f64) -> Self {
        value
    }

    fn from_usize(value: usize) -> Self {
        value as f64
    }

    fn sqrt(self) -> Self {
        self.sqrt()
    }

    fn ln(self) -> Self {
        self.ln()
    }

    fn abs(self) -> Self {
        self.abs()
    }

    fn powi(self, n: i32) -> Self {
        self.powi(n)
    }

    fn max(self, other: Self) -> Self {
        self.max(other)
    }

    fn min(self, other: Self) -> Self {
        self.min(other)
    }

    fn is_finite(self) -> bool {
        self.is_finite()
    }
}

impl Real for f32 {
    fn zero() -> Self {
        0.0
    }

    fn one() -> Self {
        1.0
    }

    fn from_f64(value: f64) -> Self {
        value as f32
    }

    fn from_usize(value: usize) -> Self {
        value as f32
    }

    fn sqrt(self) -> Self {
        self.sqrt()
    }

    fn ln(self) -> Self {
        self.ln()
    }

    fn abs(self) -> Self {
        self.abs()
    }

    fn powi(self, n: i32) -> Self {
        self.powi(n)
    }

    fn max(self, other: Self) -> Self {
        self.max(other)
    }

    fn min(self, other: Self) -> Self {
        self.min(other)
    }

    fn is_finite(self) -> bool {
        self.is_finite()
    }
}
