use super::*;

#[test]
fn fade_preserves_pixels_alpha_and_one_row_images() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("image.png");
    for height in [1, 2, 3, 5] {
        let mut original = image::RgbaImage::new(3, height);
        for y in 0..height {
            for (x, a) in [128, 0, 255].into_iter().enumerate() {
                original.put_pixel(x as u32, y, image::Rgba([10, 20, 30, a]));
            }
        }
        original.save(&path).unwrap();
        for gradient in ["none", "top-to-bottom", "bottom-to-top"] {
            for opacity in [0.0, 0.5, 1.0] {
                let png = prepare_png(&path, opacity, gradient).unwrap();
                let decoded = image::load_from_memory(&png).unwrap().into_rgba8();
                for y in 0..height {
                    let factor = if height == 1 || gradient == "none" {
                        1.0
                    } else {
                        let v = f64::from(y) / f64::from(height - 1);
                        if gradient == "top-to-bottom" {
                            1.0 - v
                        } else {
                            v
                        }
                    };
                    for (x, a) in [128, 0, 255].into_iter().enumerate() {
                        assert_eq!(
                            decoded.get_pixel(x as u32, y).0,
                            [
                                10,
                                20,
                                30,
                                (f64::from(a) * opacity * factor).round_ties_even() as u8
                            ]
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn malformed_limits_and_opacity_errors() {
    let file = tempfile::NamedTempFile::new().unwrap();
    for opacity in [-1.0, 2.0, f64::NAN, f64::INFINITY] {
        assert!(
            prepare_png(file.path(), opacity, "none")
                .unwrap_err()
                .to_string()
                .contains("opacity")
        );
    }
    assert!(
        prepare_png(file.path(), 0.2, "sideways")
            .unwrap_err()
            .to_string()
            .contains("gradient")
    );
    assert!(prepare_png(file.path(), 0.2, "none").is_err());
    assert!(prepare_png(file.path().parent().unwrap(), 0.2, "none").is_err());
    file.as_file().set_len(33 * 1024 * 1024).unwrap();
    assert!(
        prepare_png(file.path(), 0.2, "none")
            .unwrap_err()
            .to_string()
            .contains("32 MiB")
    );
}

#[test]
fn oversized_dimensions_rejected_and_thumbnail_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("image.png");
    image::RgbaImage::from_pixel(2000, 2, image::Rgba([255, 0, 0, 255]))
        .save(&path)
        .unwrap();
    let png = prepare_png(&path, 1.0, "none").unwrap();
    let image = image::load_from_memory(&png).unwrap();
    assert!(image.width() <= 1920 && image.height() <= 1080);
    image::GrayImage::new(4001, 4000).save(&path).unwrap();
    assert!(
        prepare_png(&path, 1.0, "none")
            .unwrap_err()
            .to_string()
            .contains("16 million")
    );
}
