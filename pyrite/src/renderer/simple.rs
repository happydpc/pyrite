use rand::{Rng, SeedableRng};
use rand_xorshift::XorShiftRng;

use super::algorithm::{make_tiles, Tile};
use crate::cameras::Camera;
use crate::film::{Film, Sample};
use crate::renderer::algorithm::contribute;
use crate::renderer::{Renderer, Status, WorkPool};
use crate::tracer::{trace, Light};
use crate::{
    project::program::{ExecutionContext, Resources},
    world::World,
};

pub(crate) fn render<W: WorkPool, F: FnMut(Status<'_>)>(
    film: &Film,
    workers: &mut W,
    mut on_status: F,
    renderer: &Renderer,
    world: &World,
    camera: &Camera,
    resources: Resources,
) {
    fn gen_rng() -> XorShiftRng {
        XorShiftRng::from_rng(rand::thread_rng()).expect("could not generate RNG")
    }

    let status_message = "rendering";
    on_status(Status {
        progress: 0,
        message: &status_message,
    });

    let tiles = make_tiles(film.width(), film.height(), renderer.tile_size, camera);

    let mut progress: usize = 0;
    let num_tiles = tiles.len();

    workers.do_work(
        tiles.into_iter().map(|f| (f, gen_rng())),
        |(tile, rng)| {
            render_tile(rng, tile, film, camera, world, resources, renderer);
        },
        |_, _| {
            progress += 1;
            on_status(Status {
                progress: ((progress * 100) / num_tiles) as u8,
                message: &status_message,
            });
        },
    );
}

fn render_tile<R: Rng>(
    mut rng: R,
    tile: Tile,
    film: &Film,
    camera: &Camera,
    world: &World,
    resources: Resources,
    renderer: &Renderer,
) {
    let mut additional_samples = Vec::with_capacity(renderer.spectrum_samples as usize - 1);
    let mut path = Vec::with_capacity(renderer.bounces as usize);
    let mut exe = ExecutionContext::new(resources);

    for _ in 0..(tile.area() * renderer.pixel_samples as usize) {
        additional_samples.clear();
        path.clear();

        let position = tile.sample_point(&mut rng);

        let ray = camera.ray_towards(&position, &mut rng);
        let wavelength = film.sample_wavelength(&mut rng);
        let light = Light::new(wavelength);
        trace(
            &mut path,
            &mut rng,
            ray,
            light,
            world,
            renderer.bounces,
            renderer.light_samples,
            &mut exe,
        );

        let mut main_sample = (
            Sample {
                wavelength,
                brightness: 0.0,
                weight: 1.0,
            },
            1.0,
        );

        let mut used_additional = true;
        additional_samples.extend((0..renderer.spectrum_samples - 1).map(|_| {
            (
                Sample {
                    wavelength: film.sample_wavelength(&mut rng),
                    brightness: 0.0,
                    weight: 1.0,
                },
                1.0,
            )
        }));

        for bounce in &path {
            for &mut (ref mut sample, ref mut reflectance) in &mut additional_samples {
                used_additional =
                    contribute(bounce, sample, reflectance, true, &mut exe) && used_additional;
            }

            let (ref mut sample, ref mut reflectance) = main_sample;
            contribute(bounce, sample, reflectance, false, &mut exe);
        }

        film.expose(position, main_sample.0);

        if used_additional {
            for (sample, _) in additional_samples.drain(..) {
                film.expose(position, sample);
            }
        }
    }
}
