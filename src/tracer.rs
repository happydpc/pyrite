use std;
use std::rand::Rng;
use std::sync::Arc;
use std::iter::Iterator;
use std::collections::HashMap;
use std::io::{File, BufferedReader};
use std::simd;

use cgmath::{Vector, EuclideanVector, Vector3};
use cgmath::{Ray, Ray3};
use cgmath::{Point, Point3};

use obj;
use genmesh;

use config;
use shapes;
use bkdtree;

pub type Brdf = fn(ray_in: &Vector3<f64>, ray_out: &Vector3<f64>, normal: &Vector3<f64>) -> f64;

pub trait Material {
    fn reflect(&self, wavelengths: &[f64], ray_in: &Ray3<f64>, normal: &Ray3<f64>, rng: &mut FloatRng) -> Reflection;
    fn get_emission(&self, wavelengths: &[f64], ray_in: &Vector3<f64>, normal: &Ray3<f64>, rng: &mut FloatRng) -> Option<&ParametricValue<RenderContext, f64> + Send + Sync>;
}

pub trait ParametricValue<From, To> {
    fn get(&self, i: &From) -> To; 
}

impl<From> ParametricValue<From, f64> for f64 {
    fn get(&self, _: &From) -> f64 {
        *self
    }
}

pub trait FloatRng {
    fn next_float(&mut self) -> f64;
}

impl<R: Rng> FloatRng for R {
    fn next_float(&mut self) -> f64 {
        self.gen()
    }
}

pub trait ObjectContainer {
    fn intersect(&self, ray: &Ray3<f64>) -> Option<(Ray3<f64>, &Material)>;
}

impl ObjectContainer for bkdtree::BkdTree<Arc<shapes::Shape>> {
    fn intersect(&self, ray: &Ray3<f64>) -> Option<(Ray3<f64>, &Material)> {
        let ray = BkdRay(ray);
        self.find(&ray).map(|(normal, object)| (normal, object.get_material()))
    }
}

pub struct BkdRay<'a>(pub &'a Ray3<f64>);

impl<'a> bkdtree::Ray for BkdRay<'a> {
    fn plane_intersections(&self, min: f64, max: f64, axis: uint) -> Option<(f64, f64)> {
        let &BkdRay(ray) = self;

        let (origin, direction) = match axis {
            0 => (simd::f64x2(ray.origin.x, ray.origin.x), simd::f64x2(ray.direction.x, ray.direction.x)),
            1 => (simd::f64x2(ray.origin.y, ray.origin.y), simd::f64x2(ray.direction.y, ray.direction.y)),
            _ => (simd::f64x2(ray.origin.z, ray.origin.z), simd::f64x2(ray.direction.z, ray.direction.z))
        };

        let plane = simd::f64x2(min, max);
        let simd::f64x2(min, max) = (plane - origin) / direction;
        let far = min.max(max);

        if far > 0.0 {
            let near = min.min(max);
            Some((near, far))
        } else {
            None
        }
    }

    #[inline]
    fn plane_distance(&self, min: f64, max: f64, axis: uint) -> (f64, f64) {
        let &BkdRay(ray) = self;

        let (origin, direction) = match axis {
            0 => (simd::f64x2(ray.origin.x, ray.origin.x), simd::f64x2(ray.direction.x, ray.direction.x)),
            1 => (simd::f64x2(ray.origin.y, ray.origin.y), simd::f64x2(ray.direction.y, ray.direction.y)),
            _ => (simd::f64x2(ray.origin.z, ray.origin.z), simd::f64x2(ray.direction.z, ray.direction.z))
        };

        let plane = simd::f64x2(min, max);
        let simd::f64x2(min, max) = (plane - origin) / direction;
        
        if min < max {
            (min, max)
        } else {
            (max, min)
        }
    }
}

pub enum Sky {
    Color(Box<ParametricValue<RenderContext, f64> + 'static + Send + Sync>)
}

impl Sky {
    pub fn color(&self, _direction: &Vector3<f64>) -> &ParametricValue<RenderContext, f64> {
        match *self {
            Color(ref c) => & **c,
        }
    }
}

pub struct World {
    pub sky: Sky,
    pub lights: Vec<Arc<shapes::Shape>>,
    pub objects: Box<ObjectContainer + 'static + Send + Sync>
}

impl World {
    fn intersect(&self, ray: &Ray3<f64>) -> Option<(Ray3<f64>, &Material)> {
        self.objects.intersect(ray)
    }
}

pub enum Reflection<'a> {
    Emit(&'a ParametricValue<RenderContext, f64> + Send + Sync),
    Reflect(Ray3<f64>, &'a ParametricValue<RenderContext, f64> + Send + Sync, f64, Option<Brdf>),
    Disperse(Vec<Reflection<'a>>)
}

pub struct RenderContext {
    pub wavelength: f64,
    pub normal: Vector3<f64>,
    pub incident: Vector3<f64>
}

pub struct WavelengthSample {
    pub wavelength: f64,
    reflectance: f64,
    pub brightness: f64,
    pub weight: f64,
    sample_light: bool
}

pub fn trace<R: Rng + FloatRng>(rng: &mut R, ray: Ray3<f64>, wavelengths: Vec<f64>, world: &World, bounces: uint, light_samples: uint) -> Vec<WavelengthSample> {
    let mut ray = ray;

    let mut wavelengths = wavelengths;
    let mut traced: Vec<WavelengthSample> = wavelengths.iter().map(|&wl| WavelengthSample {
        wavelength: wl,
        reflectance: 1.0,
        brightness: 0.0,
        weight: 1.0,
        sample_light: true
    }).collect();
    let mut completed = Vec::new();

    for bounce in range(0, bounces) {
        match world.intersect(&ray) {
            Some((normal, material)) => match material.reflect(wavelengths.as_slice(), &ray, &normal, &mut *rng as &mut FloatRng) {
                Reflect(out_ray, color, scale, brdf) => {
                    for sample in traced.iter_mut() {
                        let context = RenderContext {
                            wavelength: sample.wavelength,
                            normal: normal.direction,
                            incident: ray.direction
                        };

                        sample.reflectance *= color.get(&context) * scale;
                    }

                    brdf.map(|brdf| {
                        let direct_light = trace_direct(rng, light_samples, wavelengths.as_slice(), &ray.direction, &normal, world, brdf);

                        for (sample, light_sum) in traced.iter_mut().zip(direct_light.into_iter()) {
                            if light_sum > 0.0 {
                                sample.brightness += sample.reflectance * light_sum;
                                sample.sample_light = false;
                            } else {
                                sample.sample_light = true;
                            }
                        }
                    });


                    let mut i = 0;
                    while i < traced.len() {
                        let WavelengthSample {reflectance, ..} = traced[i];

                        let brdf_scale = brdf.map(|brdf| brdf(&ray.direction, &normal.direction, &out_ray.direction)).unwrap_or(1.0);
                        let new_reflectance = reflectance * brdf_scale;

                        if new_reflectance == 0.0 {
                            let sample = traced.swap_remove(i);
                            wavelengths.swap_remove(i);
                            sample.map(|sample| completed.push(sample));
                        } else {
                            let &WavelengthSample {ref mut reflectance, ref mut sample_light, ..} = traced.get_mut(i);
                            *reflectance = new_reflectance;
                            *sample_light = brdf.is_none() || *sample_light;
                            i += 1;
                        }
                    }

                    ray = out_ray;
                },
                Emit(color) => {
                    for mut sample in traced.into_iter() {
                        let context = RenderContext {
                            wavelength: sample.wavelength,
                            normal: normal.direction,
                            incident: ray.direction
                        };

                        if sample.sample_light {
                            sample.brightness += sample.reflectance * color.get(&context);
                        }
                        completed.push(sample);
                    }

                    return completed
                },
                Disperse(reflections) => {
                    let bounces = bounces - (bounce + 1);
                    for (mut sample, mut reflection) in traced.into_iter().zip(reflections.into_iter()) {
                        let context = RenderContext {
                            wavelength: sample.wavelength,
                            normal: normal.direction,
                            incident: ray.direction
                        };

                        loop {
                            match reflection {
                                Disperse(mut reflections) => reflection = reflections.pop().expect("internal error: no reflections"),
                                Reflect(out_ray, color, scale, brdf) => {
                                    sample.reflectance *= color.get(&context) * scale;
                                    
                                    brdf.map(|brdf| {
                                        let direct_light = trace_direct(rng, light_samples, [sample.wavelength].as_slice(), &ray.direction, &normal, world, brdf);
                                        let light_sum = direct_light[0];

                                        if light_sum > 0.0 {
                                            sample.brightness += sample.reflectance * light_sum;
                                            sample.sample_light = false;
                                        } else {
                                            sample.sample_light = true;
                                        }
                                    });

                                    sample.reflectance *= brdf.map(|brdf| brdf(&ray.direction, &normal.direction, &out_ray.direction)).unwrap_or(1.0);
                                    sample.sample_light = brdf.is_none() || sample.sample_light;
                                    completed.push(trace_branch(rng, out_ray, sample, world, bounces, light_samples));
                                    break;
                                },
                                Emit(color) => {
                                    if sample.sample_light {
                                        sample.brightness += sample.reflectance * color.get(&context);
                                    }
                                    completed.push(sample);
                                    break;
                                }
                            }
                        }
                    }

                    return completed;
                }
            },
            None => {
                let sky = world.sky.color(&ray.direction);
                for mut sample in traced.into_iter() {
                    let context = RenderContext {
                        wavelength: sample.wavelength,
                        normal: Vector3::new(0.0, 0.0, 0.0),
                        incident: ray.direction
                    };

                    sample.brightness += sample.reflectance * sky.get(&context);
                    completed.push(sample);
                }

                return completed
            }
        };
    }

    for sample in traced.into_iter() {
        completed.push(sample);
    }

    completed
}

fn trace_branch<R: Rng + FloatRng>(rng: &mut R, ray: Ray3<f64>, sample: WavelengthSample, world: &World, bounces: uint, light_samples: uint) -> WavelengthSample {
    let mut ray = ray;
    let mut sample = sample;
    let wl = [sample.wavelength];

    for _ in range(0, bounces) {
        match world.intersect(&ray) {
            Some((normal, material)) => {
                let mut reflection = material.reflect(wl.as_slice(), &ray, &normal, &mut *rng as &mut FloatRng);
                loop {
                    match reflection {
                        Disperse(mut reflections) => reflection = reflections.pop().expect("internal error: no reflections in branch"),
                        Reflect(out_ray, color, scale, brdf) => {
                            let context = RenderContext {
                                wavelength: sample.wavelength,
                                normal: normal.direction,
                                incident: ray.direction
                            };

                            sample.reflectance *= color.get(&context) * scale;

                            brdf.map(|brdf| {
                                let direct_light = trace_direct(rng, light_samples, wl.as_slice(), &ray.direction, &normal, world, brdf);
                                let light_sum = direct_light[0];
                                
                                if light_sum > 0.0 {
                                    sample.brightness += sample.reflectance * light_sum;
                                    sample.sample_light = false;
                                } else {
                                    sample.sample_light = true;
                                }
                            });

                            sample.reflectance *= brdf.map(|brdf| brdf(&ray.direction, &normal.direction, &out_ray.direction)).unwrap_or(1.0);
                            sample.sample_light = brdf.is_none() || sample.sample_light;

                            if sample.reflectance == 0.0 {
                                return sample;
                            }

                            ray = out_ray;
                            break;
                        },
                        Emit(color) => {
                            let context = RenderContext {
                                wavelength: sample.wavelength,
                                normal: normal.direction,
                                incident: ray.direction
                            };
                            if sample.sample_light {
                                sample.brightness += sample.reflectance * color.get(&context);
                            }
                            return sample;
                        }
                    }
                }
            },
            None => {
                let sky = world.sky.color(&ray.direction);
                
                let context = RenderContext {
                    wavelength: sample.wavelength,
                    normal: Vector3::new(0.0, 0.0, 0.0),
                    incident: ray.direction
                };

                sample.brightness += sample.reflectance * sky.get(&context);
                return sample
            }
        };
    }

    sample
}

fn trace_direct<'a, R: Rng + FloatRng>(rng: &mut R, samples: uint, wavelengths: &[f64], ray_in: &Vector3<f64>, normal: &Ray3<f64>, world: &'a World, brdf: Brdf) -> Vec<f64> {
    if world.lights.len() == 0 {
        return Vec::from_elem(samples as uint, 0.0f64);
    }

    let n = if ray_in.dot(&normal.direction) < 0.0 {
        normal.direction
    } else {
        -normal.direction
    };

    let normal = Ray::new(normal.origin, n);

    let ref light = world.lights[rng.gen_range(0, world.lights.len())];
    let weight = light.surface_area() * world.lights.len() as f64 / (samples as f64 * 2.0 * std::f64::consts::PI);

    range(0, samples).fold(Vec::from_elem(samples as uint, 0.0f64), |mut sum, _| {
        let target_normal = light.sample_point(rng);
        let ray_out = target_normal.origin.sub_p(&normal.origin);
        let distance = ray_out.length2();
        let ray_out = Ray::new(normal.origin, ray_out.normalize());

        let cos_out = normal.direction.dot(&ray_out.direction).max(0.0);
        let cos_in = target_normal.direction.dot(& -ray_out.direction).abs();

        if cos_out > 0.0 {
            let color = light.get_material().get_emission(wavelengths, &ray_out.direction, &target_normal, &mut *rng as &mut FloatRng);
            let scale = weight * cos_in * brdf(ray_in, &normal.direction, &ray_out.direction) / distance;

            color.map(|color| match world.intersect(&ray_out) {
                None => for (&wavelength, mut sum) in wavelengths.iter().zip(sum.iter_mut()) {
                    let context = RenderContext {
                        wavelength: wavelength,
                        normal: target_normal.direction,
                        incident: ray_out.direction
                    };

                    *sum += color.get(&context) * scale;
                },
                Some((hit_normal, _)) if hit_normal.origin.sub_p(&normal.origin).length2() >= distance - 0.0000001
                  => for (&wavelength, mut sum) in wavelengths.iter().zip(sum.iter_mut()) {
                    let context = RenderContext {
                        wavelength: wavelength,
                        normal: target_normal.direction,
                        incident: ray_out.direction
                    };

                    *sum += color.get(&context) * scale;
                },
                _ => {}
            });
        }
        
        sum
    })
}



pub fn register_types(context: &mut config::ConfigContext) {
    context.insert_grouped_type("Sky", "Color", decode_sky_color);
}

fn decode_sky_color(context: &config::ConfigContext, fields: HashMap<String, config::ConfigItem>) -> Result<Sky, String> {
    let mut fields = fields;

    let color = match fields.pop_equiv(&"color") {
        Some(v) => try!(decode_parametric_number(context, v), "color"),
        None => return Err(String::from_str("missing field 'color'"))
    };

    Ok(Color(color))
}

pub fn decode_world(context: &config::ConfigContext, item: config::ConfigItem, make_path: |String| -> Path) -> Result<World, String> {
    match item {
        config::Structure(_, mut fields) => {
            let sky = match fields.pop_equiv(&"sky") {
                Some(v) => try!(context.decode_structure_from_group("Sky", v), "sky"),
                None => return Err(String::from_str("missing field 'sky'"))
            };

            let object_protos = match fields.pop_equiv(&"objects") {
                Some(v) => try!(v.into_list(), "objects"),
                None => return Err(String::from_str("missing field 'objects'"))
            };

            let mut objects: Vec<Arc<shapes::Shape>> = Vec::new();
            let mut lights: Vec<Arc<shapes::Shape>> = Vec::new();

            for (i, object) in object_protos.into_iter().enumerate() {
                let shape: shapes::ProxyShape = try!(context.decode_structure_from_group("Shape", object), format!("objects: [{}]", i));
                match shape {
                    shapes::DecodedShape { shape, emissive } => {
                        let shape = Arc::new(shape);
                        if emissive {
                            lights.push(shape.clone());
                        }
                        objects.push(shape);
                    },
                    shapes::Mesh { file, mut materials } => {
                        let path = make_path(file);
                        let mut file = BufferedReader::new(File::open(&path));
                        let obj = obj::Obj::load(&mut file);
                        for object in obj.object_iter() {
                            println!("adding object '{}'", object.name);
                            
                            let (object_material, emissive) = match materials.pop_equiv(&object.name) {
                                Some(v) => {
                                    let (material, emissive): (Box<Material + 'static + Send + Sync>, bool) =
                                        try!(context.decode_structure_from_group("Material", v));

                                    (Arc::new(material), emissive)
                                },
                                None => return Err(format!("objects: [{}]: missing field '{}'", i, object.name))
                            };

                            for group in object.group_iter() {
                                for shape in group.indices().iter() {
                                    match *shape {
                                        genmesh::PolyTri(genmesh::Triangle{
                                            x: (v1, _t1, _n1),
                                            y: (v2, _t2, _n2),
                                            z: (v3, _t3, _n3)
                                        }) => {
                                            let triangle = Arc::new(shapes::Triangle {
                                                v1: convert_vertex(&obj.position()[v1]),
                                                v2: convert_vertex(&obj.position()[v2]),
                                                v3: convert_vertex(&obj.position()[v3]),
                                                material: object_material.clone()
                                            });

                                            if emissive {
                                                lights.push(triangle.clone());
                                            }

                                            objects.push(triangle);
                                        },
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                }
            }

            println!("the scene contains {} objects", objects.len())
            println!("building BKD-Tree... ")
            let tree = bkdtree::BkdTree::new(objects, 3);
            println!("done building BKD-Tree")
            Ok(World {
                sky: sky,
                lights: lights,
                objects: box tree as Box<ObjectContainer + 'static + Send + Sync>
            })
        },
        config::Primitive(v) => Err(format!("unexpected {}", v)),
        config::List(_) => Err(format!("unexpected list"))
    }
}

fn convert_vertex(&[x, y, z]: &[f32, ..3]) -> Point3<f64> {
    Point3::new(x as f64, y as f64, z as f64)
}

pub fn decode_parametric_number<From>(context: &config::ConfigContext, item: config::ConfigItem) -> Result<Box<ParametricValue<From, f64> + 'static + Send + Sync>, String> {
    let group_names = vec!["Math", "Value"];

    let name_collection = match group_names.as_slice() {
        [name] => format!("'{}'", name),
        [names.., last] => format!("'{}' or '{}'", names.connect("', '"), last),
        [] => return Err(String::from_str("internal error: trying to decode structure from one of 0 groups"))
    };

    match item {
        config::Structure(..) => context.decode_structure_from_groups(group_names, item),
        config::Primitive(config::parser::Number(n)) => Ok(box n as Box<ParametricValue<From, f64> + 'static + Send + Sync>),
        v => return Err(format!("expected a number or a structure from group {}, but found {}", name_collection, v))
    }
}