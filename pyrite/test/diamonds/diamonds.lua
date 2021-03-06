local diamond = {
    surface = material.refractive {
        ior = 2.37782,
        dispersion = 0.01371,
        color = 1,
    },
}

local plexi = {surface = material.mirror {color = mix(0, 0.2, fresnel(1.1))}}

return {
    image = {width = 512, height = 300},

    renderer = renderer.simple {
        pixel_samples = 200,
        spectrum_samples = 1,
        spectrum_bins = 50,
        tile_size = 32,
        bounces = 256,
    },

    camera = camera.perspective {
        fov = 12.5,
        transform = transform.look_at {
            from = vector(-6.55068, -8.55076, 4.0),
            to = vector(0.1, 0, 0.1),
            up = vector {z = 1},
        },
        focus_distance = 11.08,
        aperture = 0.02,
    },

    world = {
        objects = {
            shape.mesh {
                file = "diamonds.obj",

                materials = {
                    diamonds = diamond,
                    light_left = {
                        surface = material.emission {color = light_source.d65},
                    },
                    light_right = {
                        surface = material.emission {
                            color = light_source.d65 * 2,
                        },
                    },
                    bottom = plexi,
                },
            },
        },
    },
}
