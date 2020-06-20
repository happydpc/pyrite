use std::{convert::TryFrom, error::Error, path::PathBuf};

use crate::tracer;

use crate::project::{ComplexExpression, Expression, FromComplexExpression, FromExpression};

pub const DIST_EPSILON: f32 = 0.0001;

macro_rules! make_operators {
    ($($struct_name:ident { $($arg:ident),+ } => $operation:expr),*) => (
        pub enum Operator<T> {
            $(
                $struct_name($struct_name<T>),
            )*
        }

        impl<T: tracer::ParametricValue<From, f32>, From> tracer::ParametricValue<From, f32> for Operator<T> {
            fn get(&self, i: &From) -> f32 {
                match self {
                    $(
                        Operator::$struct_name(op) => op.get(i),
                    )+
                }
            }
        }

        $(
            pub struct $struct_name<T> {
                $(
                    $arg: Box<T>
                ),+
            }

            impl<T: tracer::ParametricValue<From, f32>, From> tracer::ParametricValue<From, f32> for $struct_name<T> {
                fn get(&self, i: &From) -> f32 {
                    $(
                        let $arg = self.$arg.get(i);
                    )+
                    $operation
                }
            }
        )*
    )
}

pub mod utils {
    use std;

    use rand::Rng;

    use super::DIST_EPSILON;
    use cgmath::{InnerSpace, Vector3};

    pub struct Interpolated<T = Vec<(f32, f32)>> {
        pub points: T,
    }

    impl<P: AsRef<[(f32, f32)]>> Interpolated<P> {
        pub fn get(&self, input: f32) -> f32 {
            let points = self.points.as_ref();
            if points.len() == 0 {
                return 0.0;
            }

            let mut min = 0;
            let mut max = points.len() - 1;

            let (min_x, _min_y) = points[min];

            if min_x >= input {
                return 0.0; // min_y
            }

            let (max_x, _max_y) = points[max];

            if max_x <= input {
                return 0.0; // max_y
            }

            while max > min + 1 {
                let check = (max + min) / 2;
                let (check_x, check_y) = points[check];

                if check_x == input {
                    return check_y;
                }

                if check_x > input {
                    max = check
                } else {
                    min = check
                }
            }

            let (min_x, min_y) = points[min];
            let (max_x, max_y) = points[max];

            if input < min_x {
                0.0 //min_y
            } else if input > max_x {
                0.0 //max_y
            } else {
                min_y + (max_y - min_y) * (input - min_x) / (max_x - min_x)
            }
        }

        pub fn segments_between(&self, min: f32, max: f32, segments: usize) -> Segments<'_> {
            Segments::new(self.points.as_ref().iter(), min, max, segments)
        }
    }

    pub struct Segments<'a> {
        from: f32,
        segment_size: f32,
        segments: usize,
        current_segment: usize,
        around_start: (Option<(f32, f32)>, Option<(f32, f32)>),
        around_end: (Option<(f32, f32)>, Option<(f32, f32)>),
        points: std::iter::Peekable<std::slice::Iter<'a, (f32, f32)>>,
    }

    impl<'a> Segments<'a> {
        fn new(
            points: std::slice::Iter<'a, (f32, f32)>,
            min: f32,
            max: f32,
            segments: usize,
        ) -> Self {
            if segments < 1 {
                panic!("need at least one segment");
            }
            let mut points = points.peekable();

            let segment_size = (max - min) / segments as f32;

            let start = min;
            let end = min + segment_size;
            let mut start_before = None;
            let mut start_after = None;
            let mut end_before = None;
            let mut end_after = None;

            while let Some(&&(x, y)) = points.peek() {
                if x >= end && end_after.is_some() {
                    break;
                }

                points.next();

                if x <= start {
                    start_before = Some((x, y));
                }

                if x >= start {
                    start_after = Some((x, y));
                }

                if x <= end {
                    end_before = Some((x, y));
                }

                if x >= end {
                    end_after = Some((x, y));
                }
            }

            Segments {
                from: min,
                segment_size,
                segments,
                current_segment: 0,
                around_start: (start_before, start_after),
                around_end: (end_before, end_after),
                points,
            }
        }
    }

    impl<'a> Iterator for Segments<'a> {
        type Item = ((f32, f32), (f32, f32));

        fn next(&mut self) -> Option<Self::Item> {
            if self.current_segment >= self.segments {
                return None;
            }

            let start = self.current_segment as f32 * self.segment_size + self.from;
            let end = (self.current_segment + 1) as f32 * self.segment_size + self.from;

            let start_value = match self.around_start {
                (Some((x1, y1)), Some((x2, y2))) => {
                    if x1 == x2 {
                        Some(y1)
                    } else {
                        let width = x2 - x1;
                        let point = (start - x1) / width;

                        Some((1.0 - point) * y1 + point * y2)
                    }
                }
                (Some((_, y)), None) | (None, Some((_, y))) => Some(y),
                (None, None) => None,
            };

            let end_value = match self.around_end {
                (Some((x1, y1)), Some((x2, y2))) => {
                    if x1 == x2 {
                        Some(y1)
                    } else {
                        let width = x2 - x1;
                        let point = (end - x1) / width;

                        Some((1.0 - point) * y1 + point * y2)
                    }
                }
                (Some((_, y)), None) | (None, Some((_, y))) => Some(y),
                (None, None) => None,
            };

            let result = match (start_value, end_value) {
                (Some(y1), Some(y2)) => Some(((start, y1), (end, y2))),
                (Some(y), None) => Some(((start, y), (end, y))),
                (None, Some(y)) => Some(((start, y), (end, y))),
                (None, None) => None,
            };

            self.current_segment += 1;
            self.around_start = self.around_end;

            let next_end = (self.current_segment + 1) as f32 * self.segment_size + self.from;

            let find_new_end = if let Some((x, _)) = self.around_end.1 {
                x < next_end
            } else {
                false
            };

            if find_new_end {
                let mut end_before = None;
                let mut end_after = None;

                while let Some(&&(x, y)) = self.points.peek() {
                    if x >= next_end && end_after.is_some() {
                        break;
                    }

                    self.points.next();

                    if x <= next_end {
                        end_before = Some((x, y));
                    }

                    if x >= next_end {
                        end_after = Some((x, y));
                    }
                }

                self.around_end = (end_before, end_after);
            }

            result
        }
    }

    pub fn schlick(
        ref_index1: f32,
        ref_index2: f32,
        normal: Vector3<f32>,
        incident: Vector3<f32>,
    ) -> f32 {
        let mut cos_psi = -normal.dot(incident);
        let r0 = (ref_index1 - ref_index2) / (ref_index1 + ref_index2);

        if ref_index1 > ref_index2 {
            let n = ref_index1 / ref_index2;
            let sin_t2 = n * n * (1.0 - cos_psi * cos_psi);
            if sin_t2 > 1.0 {
                return 1.0;
            }
            cos_psi = (1.0 - sin_t2).sqrt();
        }

        let inv_cos = 1.0 - cos_psi;

        return r0 * r0 + (1.0 - r0 * r0) * inv_cos * inv_cos * inv_cos * inv_cos * inv_cos;
    }

    pub fn ortho(v: Vector3<f32>) -> Vector3<f32> {
        let unit = if v.x.abs() < DIST_EPSILON {
            Vector3::unit_x()
        } else if v.y.abs() < DIST_EPSILON {
            Vector3::unit_y()
        } else if v.z.abs() < DIST_EPSILON {
            Vector3::unit_z()
        } else {
            Vector3 {
                x: -v.y,
                y: v.x,
                z: 0.0,
            }
        };

        v.cross(unit)
    }

    pub fn sample_cone<R: ?Sized + Rng>(
        rng: &mut R,
        direction: Vector3<f32>,
        cos_half: f32,
    ) -> Vector3<f32> {
        let o1 = ortho(direction).normalize();
        let o2 = direction.cross(o1).normalize();
        let r1: f32 = std::f32::consts::PI * 2.0 * rng.gen::<f32>();
        let r2: f32 = cos_half + (1.0 - cos_half) * rng.gen::<f32>();
        let oneminus = (1.0 - r2 * r2).sqrt();

        o1 * r1.cos() * oneminus + o2 * r1.sin() * oneminus + &direction * r2
    }

    pub fn solid_angle(cos_half: f32) -> f32 {
        if cos_half >= 1.0 {
            0.0
        } else {
            2.0 * std::f32::consts::PI * (1.0 - cos_half)
        }
    }

    pub fn sample_sphere<R: ?Sized + Rng>(rng: &mut R) -> Vector3<f32> {
        let u = rng.gen::<f32>();
        let v = rng.gen::<f32>();
        let theta = 2.0 * std::f32::consts::PI * u;
        let phi = (2.0 * v - 1.0).acos();
        Vector3::new(phi.sin() * theta.cos(), phi.sin() * theta.sin(), phi.cos())
    }

    pub fn sample_hemisphere<R: ?Sized + Rng>(
        rng: &mut R,
        direction: Vector3<f32>,
    ) -> Vector3<f32> {
        let s = sample_sphere(rng);
        let x = ortho(direction).normalize_to(s.x);
        let y = x.cross(direction).normalize_to(s.y);
        let z = direction.normalize_to(s.z.abs());
        x + y + z
    }
}

make_operators! {
    Add { a, b }         => a + b,
    Sub { a, b }         => a - b,
    Mul { a, b }         => a * b,
    Div { a, b }         => a / b,
    // Abs { a }            => a.abs(),
    // Min { a, b }         => a.min(b),
    // Max { a, b }         => a.max(b),
    Mix { a, b, factor } => { let f = factor.min(1.0).max(0.0); a * (1.0 - f) + b * f }
}

enum BinaryOperator {
    Add,
    Sub,
    Mul,
    Div,
}

impl<T: FromExpression> Operator<T> {
    fn from_binary(
        operator: BinaryOperator,
        a: Expression,
        b: Expression,
        make_path: &impl Fn(&str) -> PathBuf,
    ) -> Result<Self, Box<dyn Error>> {
        Ok(match operator {
            BinaryOperator::Add => Operator::Add(Add {
                a: Box::new(a.parse(make_path)?),
                b: Box::new(b.parse(make_path)?),
            }),
            BinaryOperator::Sub => Operator::Sub(Sub {
                a: Box::new(a.parse(make_path)?),
                b: Box::new(b.parse(make_path)?),
            }),
            BinaryOperator::Mul => Operator::Mul(Mul {
                a: Box::new(a.parse(make_path)?),
                b: Box::new(b.parse(make_path)?),
            }),
            BinaryOperator::Div => Operator::Div(Div {
                a: Box::new(a.parse(make_path)?),
                b: Box::new(b.parse(make_path)?),
            }),
        })
    }
}

pub enum Math<T, E = NoOp> {
    Value(T),
    Operator(Operator<Math<T, E>>),
    Curve(Curve<T>),
    Extra(E),
}

impl<T: tracer::ParametricValue<From, f32>, E: tracer::ParametricValue<From, f32>, From>
    tracer::ParametricValue<From, f32> for Math<T, E>
{
    fn get(&self, i: &From) -> f32 {
        match self {
            Math::Value(value) => value.get(i),
            Math::Operator(op) => op.get(i),
            Math::Curve(curve) => curve.get(i),
            Math::Extra(extra) => extra.get(i),
        }
    }
}

impl<T: From<f32>, E> From<f32> for Math<T, E> {
    fn from(constant: f32) -> Self {
        Math::Value(constant.into())
    }
}

impl<T, E> FromExpression for Math<T, E>
where
    T: From<f32> + FromComplexExpression,
    E: FromComplexExpression,
{
    fn from_expression(
        expression: Expression,
        make_path: &impl Fn(&str) -> PathBuf,
    ) -> Result<Self, Box<dyn Error>> {
        match expression {
            Expression::Number(number) => Ok(Self::from(number as f32)),
            Expression::Complex(ComplexExpression::Add { a, b }) => Ok(Math::Operator(
                Operator::from_binary(BinaryOperator::Add, *a, *b, make_path)?,
            )),
            Expression::Complex(ComplexExpression::Sub { a, b }) => Ok(Math::Operator(
                Operator::from_binary(BinaryOperator::Sub, *a, *b, make_path)?,
            )),
            Expression::Complex(ComplexExpression::Mul { a, b }) => Ok(Math::Operator(
                Operator::from_binary(BinaryOperator::Mul, *a, *b, make_path)?,
            )),
            Expression::Complex(ComplexExpression::Div { a, b }) => Ok(Math::Operator(
                Operator::from_binary(BinaryOperator::Div, *a, *b, make_path)?,
            )),
            Expression::Complex(ComplexExpression::Mix { a, b, factor }) => {
                Ok(Math::Operator(Operator::Mix(Mix {
                    a: Box::new(a.parse(make_path)?),
                    b: Box::new(b.parse(make_path)?),
                    factor: Box::new(factor.parse(make_path)?),
                })))
            }
            Expression::Complex(complex) => E::from_complex_expression(complex.clone(), make_path)
                .map(Math::Extra)
                .or_else(|_| T::from_complex_expression(complex, make_path).map(Math::Value)),
            Expression::Boolean(_) => Err("Boolean values cannot be used in this context".into()),
        }
    }
}

pub type RenderMath<T> = Math<T, Fresnel<T>>;

pub struct NoOp;

impl TryFrom<ComplexExpression> for NoOp {
    type Error = Box<dyn Error>;

    fn try_from(value: ComplexExpression) -> Result<Self, Self::Error> {
        Err(match value {
            ComplexExpression::Vector { .. } => "vectors cannot be used in this context".into(),
            ComplexExpression::Fresnel { .. } => "Fresnel cannot be used in this context".into(),
            ComplexExpression::LightSource { .. } => {
                "light sources cannot be used in this context".into()
            }
            ComplexExpression::Spectrum { .. } => "spectra cannot be used in this context".into(),
            ComplexExpression::Rgb { .. } => "RGB cannot be used in this context".into(),
            ComplexExpression::Texture { .. } => "textures cannot be used in this context".into(),
            ComplexExpression::Add { .. } => "addition cannot be used in this context".into(),
            ComplexExpression::Sub { .. } => "subtraction cannot be used in this context".into(),
            ComplexExpression::Mul { .. } => "multiplication cannot be used in this context".into(),
            ComplexExpression::Div { .. } => "division cannot be used in this context".into(),
            ComplexExpression::Mix { .. } => "mix cannot be used in this context".into(),
        })
    }
}

impl<From> tracer::ParametricValue<From, f32> for NoOp {
    fn get(&self, _: &From) -> f32 {
        0.0
    }
}

pub struct Curve<T> {
    input: Box<Math<T>>,
    points: utils::Interpolated,
}

impl<T: tracer::ParametricValue<From, f32>, From> tracer::ParametricValue<From, f32> for Curve<T> {
    fn get(&self, i: &From) -> f32 {
        self.points.get(self.input.get(i))
    }
}

pub struct Fresnel<T> {
    ior: Box<RenderMath<T>>,
    env_ior: Box<RenderMath<T>>,
}

impl<T: tracer::ParametricValue<tracer::RenderContext, f32>>
    tracer::ParametricValue<tracer::RenderContext, f32> for Fresnel<T>
{
    fn get(&self, i: &tracer::RenderContext) -> f32 {
        use cgmath::InnerSpace;

        let normal = i.normal;
        let incident = i.incident;

        if incident.dot(normal) < 0.0 {
            utils::schlick(self.env_ior.get(i), self.ior.get(i), normal, incident)
        } else {
            utils::schlick(self.ior.get(i), self.env_ior.get(i), -normal, incident)
        }
    }
}

impl<T> FromComplexExpression for Fresnel<T>
where
    T: From<f32> + FromComplexExpression,
{
    fn from_complex_expression(
        value: ComplexExpression,
        make_path: &impl Fn(&str) -> PathBuf,
    ) -> Result<Self, Box<dyn Error>> {
        match value {
            ComplexExpression::Fresnel { ior, env_ior } => Ok(Fresnel {
                ior: Box::new(ior.parse(make_path)?),
                env_ior: Box::new(RenderMath::from_expression_or_else(
                    env_ior.map(|e| *e),
                    make_path,
                    || 1.0f32.into(),
                )?),
            }),
            _ => Err("expected a Fresnel expression".into()),
        }
    }
}
