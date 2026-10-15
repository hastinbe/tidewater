//! The screen's layers as pictures the core owns (`docs/design/client-core.md`, migration step 3): each layer is a
//! picture in [`Pictures`] and a [`Layer`] that says where it goes on the screen. They replace the sprites and image
//! assets the layers used to be; nothing here depends on an engine.
//!
//! A layer is on one of three planes, composed in this order (`shell`):
//!
//! | Plane | Placed |
//! |---|---|
//! | [`Plane::Map`] | In the map view: its centre in map pixels (y down), drawn at the view's zoom |
//! | [`Plane::Interface`] | Centred on the screen, one picture pixel per screen pixel |
//! | [`Plane::Flow`] | Its centre this far from the screen's centre (y down), at its size, with its grey tint |
//!
//! Within a plane the layers go back to front by `z`, and in the order they were made where `z` is equal.

use std::collections::HashMap;

use crate::scene::Picture;

/// A picture in [`Pictures`]. Ids are never used again, so a stale one finds nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PictureId(u64);

/// The layers' pictures.
#[derive(Default)]
pub struct Pictures {
    next: u64,
    pictures: HashMap<PictureId, Picture>,
}

impl Pictures {
    pub fn add(&mut self, p: Picture) -> PictureId {
        let id = PictureId(self.next);
        self.next += 1;
        self.pictures.insert(id, p);
        id
    }

    pub fn get(&self, id: PictureId) -> Option<&Picture> {
        self.pictures.get(&id)
    }

    pub fn get_mut(&mut self, id: PictureId) -> Option<&mut Picture> {
        self.pictures.get_mut(&id)
    }

    /// Replaces picture `id`, if it is still there.
    pub fn set(&mut self, id: PictureId, p: Picture) {
        if let Some(old) = self.pictures.get_mut(&id) {
            *old = p;
        }
    }

    pub fn remove(&mut self, id: PictureId) -> Option<Picture> {
        self.pictures.remove(&id)
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.pictures.len()
    }
}

/// Which part of the screen a layer belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plane {
    Map,
    Interface,
    Flow,
}

/// One layer of the screen: a picture and its place.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layer {
    pub picture: PictureId,
    pub plane: Plane,
    /// Back to front within the plane.
    pub z: f32,
    /// The picture's centre: in map pixels on the map plane, from the screen's centre on the flow plane (y down on
    /// both); unused on the interface plane.
    pub at: (f32, f32),
    /// The size it is drawn at, in map or screen pixels; `None`: the picture's own size.
    pub size: Option<(f32, f32)>,
    /// A grey tint (sRGB, 0 to 1): 1 leaves the colours as they are.
    pub tint: f32,
    pub visible: bool,
}

impl Layer {
    pub fn new(picture: PictureId, plane: Plane, z: f32) -> Layer {
        Layer {
            picture,
            plane,
            z,
            at: (0.0, 0.0),
            size: None,
            tint: 1.0,
            visible: true,
        }
    }

    pub fn at(self, x: f32, y: f32) -> Layer {
        Layer { at: (x, y), ..self }
    }

    #[cfg(test)]
    pub fn sized(self, w: f32, h: f32) -> Layer {
        Layer {
            size: Some((w, h)),
            ..self
        }
    }

    /// The size it is drawn at, for a picture `pic`.
    pub fn drawn_size(&self, pic: &Picture) -> (f32, f32) {
        self.size.unwrap_or((pic.width as f32, pic.height as f32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pic(w: u32, h: u32) -> Picture {
        Picture {
            width: w,
            height: h,
            rgba: vec![0; (w * h * 4) as usize],
        }
    }

    #[test]
    fn ids_are_never_used_again() {
        let mut p = Pictures::default();
        let a = p.add(pic(1, 1));
        assert!(p.remove(a).is_some());
        let b = p.add(pic(2, 2));
        assert_ne!(a, b);
        assert!(p.get(a).is_none());
        p.set(a, pic(3, 3));
        assert!(p.get(a).is_none(), "a removed picture stays removed");
        assert_eq!(p.len(), 1);
    }

    #[test]
    fn a_layer_is_drawn_at_its_picture_size_unless_sized() {
        let p = pic(4, 3);
        let id = Pictures::default().add(pic(4, 3));
        let l = Layer::new(id, Plane::Flow, 1.0);
        assert_eq!(l.drawn_size(&p), (4.0, 3.0));
        assert_eq!(l.sized(8.0, 6.0).drawn_size(&p), (8.0, 6.0));
        assert_eq!(l.at(2.0, -1.0).at, (2.0, -1.0));
    }
}
