use sefer_alloc::registry::segment_route::RoutePin;

fn small(pin: RoutePin) {
    let _ = pin.publish_small(16);
}

fn large(pin: RoutePin) {
    let _ = pin.publish_large();
}

fn main() {}
