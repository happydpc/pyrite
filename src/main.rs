#![feature(macro_rules, struct_variant)]

extern crate cgmath;
extern crate image;

use std::sync::{TaskPool, Arc, RWLock};
use std::io::File;

use cgmath::vector::{Vector2, Vector3};
use cgmath::rotation::Rotation;
use cgmath::transform::Decomposed;
use cgmath::ray::Ray3;

use tracer::Material;

use renderer::Tile;

macro_rules! try(
    ($e:expr) => (
        match $e {
            Ok(v) => v,
            Err(e) => return Err(e)
        }
    );

    ($e:expr, $under:expr) => (
        match $e {
            Ok(v) => v,
            Err(e) => return Err(format!("{}: {}", $under, e))
        }
    )
)

mod tracer;
mod cameras;
mod worlds;
mod shapes;
mod materials;
mod config;
mod project;
mod renderer;
mod types3d;

fn main() {
    let args = std::os::args();

    if args.len() > 1 {
        match project::from_file(Path::new(args[1].clone())) {
            project::Success(p) => render(p),
            project::IoError(e) => println!("error while reading project file: {}", e),
            project::ParseError(e) => println!("error while parsing project file: {}", e)
        }
    } else {
        println!("usage: {} project_file", args[0]);
    }
}

fn render(project: project::Project) {
    let image_size = Vector2::new(project.image.width, project.image.height);

    let tiles = project.renderer.make_tiles(&project.camera, &image_size);
    let tile_count = tiles.len();

    let sphere1 = shapes::Sphere(
        Decomposed {
            scale: 1.0,
            rot: Rotation::identity(),
            disp: Vector3::new(0.0, 0.0, -6.0)
        }
    );

    let sphere2 = shapes::Sphere(
        Decomposed {
            scale: 1.0,
            rot: Rotation::identity(),
            disp: Vector3::new(2.0, 0.0, -6.0)
        }
    );

    let config = Arc::new(RenderContext {
        camera: project.camera,
        world: worlds::SimpleWorld::new(vec![Geometric(sphere1, box materials::Diffuse {reflection: 0.8f64}), Geometric(sphere2, box materials::Emission {spectrum: 1.0f64})], 0.0f64),
        pending: RWLock::new(tiles),
        completed: RWLock::new(Vec::new()),
        renderer: project.renderer
    });

    let mut pool = TaskPool::new(project.renderer.threads(), || {
        let config = config.clone();
        proc(id: uint) {
            (id, config)
        }
    });

    for _ in range(0, tile_count) {
        pool.execute(proc(&(task_id, ref context): &(uint, Arc<RenderContext<worlds::SimpleWorld<Vec<Object>, f64>>>)) {
            let mut tile = {
                context.pending.write().pop().unwrap()
            };
            println!("Task {} got tile {}", task_id, tile.screen_area().from);

            //tracer::render(&mut tile, samples, &context.camera, &context.world, context.depth, &context.shared_stats);
            context.renderer.render_tile(&mut tile, &context.camera, &context.world);

            context.completed.write().push(tile);
        })
    }

    let mut tile_counter = 0;

    let mut pixels = Vec::from_elem(image_size.x * image_size.y * 3, 0);
    
    while tile_counter < tile_count {
        std::io::timer::sleep(4000);


        loop {
            match config.completed.write().pop() {
                Some(tile) => {
                    for (spectrum, position) in tile.pixels() {
                        let value = clamp_channel(spectrum.value_at(0.0));
                        *pixels.get_mut(position.x * 3 + position.y * image_size.x * 3)     = value;
                        *pixels.get_mut(position.x * 3 + position.y * image_size.x * 3 + 1) = value;
                        *pixels.get_mut(position.x * 3 + position.y * image_size.x * 3 + 2) = value;
                    }

                    tile_counter += 1;
                },
                None => break
            }
        }

        let mut encoder = image::PNGEncoder::new(File::create(&Path::new("test.png")));
        match encoder.encode(pixels.as_slice(), image_size.x as u32, image_size.y as u32, image::RGB(8)) {
            Err(e) => println!("error while writing image: {}", e),
            _ => {}
        }
    }

    println!("Done!")
}

struct RenderContext<W> {
    camera: cameras::Camera,
    world: W,
    pending: RWLock<Vec<Tile>>,
    completed: RWLock<Vec<Tile>>,
    renderer: renderer::Renderer
}

enum Object {
    Geometric(shapes::Shape, Box<Material + Send + Share>)
}

impl worlds::WorldObject for Object {
    fn intersect(&self, ray: &Ray3<f64>) -> Option<(Ray3<f64>, &Material)> {
        match *self {
            Geometric(shape, ref material) => {
                shape.intersect(ray).map(|r| (r, material as &Material))
            }
        }
    }
}

fn clamp_channel(value: f64) -> u8 {
    if value >= 1.0 {
        255
    } else if value <= 0.0 {
        0
    } else {
        (value * 255.0) as u8
    }
}