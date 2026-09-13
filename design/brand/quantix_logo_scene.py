"""Quantix premium 3D logo scene builder.

This module only creates a new Blender scene.  It deliberately leaves the
current/default scene, rendering, and saving to the caller (the Blender MCP
operator used by the parent task).

Public entry point::

    result = build_scene()

The returned dictionary is intentionally plain and compact so it can be
passed through an MCP result without serialising Blender data blocks.
"""

import bpy
import bmesh
from mathutils import Vector
import math
import os


# These globals are useful to an interactive Blender/MCP caller after
# ``build_scene`` has completed.
root = None
emblem_collection = None
wordmark_collection = None
studio_collection = None
camera = None
icon_camera = None


OUTPUT_DIR = r"C:\Users\kareem\.quantix\outputs\brand\quantix-3d-2026-09-09"
HERO_OUTPUT = os.path.join(OUTPUT_DIR, "quantix-hero-preview.png")
FONT_CANDIDATES = (
    r"C:\Windows\Fonts\bahnschrift.ttf",
    r"C:\Windows\Fonts\segoeuib.ttf",
)


def _unique_name(mapping, requested):
    """Return a stable requested name, suffixing only on an existing clash."""
    if mapping.get(requested) is None:
        return requested
    index = 1
    while mapping.get("%s.%03d" % (requested, index)) is not None:
        index += 1
    return "%s.%03d" % (requested, index)


def _new_scene():
    scene_name = _unique_name(bpy.data.scenes, "Quantix | Atelier")
    scene = bpy.data.scenes.new(scene_name)
    return scene


def _new_collection(scene, requested):
    collection_name = _unique_name(bpy.data.collections, requested)
    collection = bpy.data.collections.new(collection_name)
    scene.collection.children.link(collection)
    return collection


def _link(collection, obj):
    collection.objects.link(obj)
    return obj


def _material_principled(name, base_color, metallic, roughness, coat=0.0):
    material = bpy.data.materials.new(name)
    material.use_nodes = True
    material.diffuse_color = (*base_color, 1.0)
    nodes = material.node_tree.nodes
    principled = nodes.get("Principled BSDF")
    if principled is None:
        return material

    def set_input(names, value):
        for socket_name in names:
            socket = principled.inputs.get(socket_name)
            if socket is not None:
                socket.default_value = value
                return

    set_input(("Base Color",), (*base_color, 1.0))
    set_input(("Metallic",), metallic)
    set_input(("Roughness",), roughness)
    set_input(("Coat Weight", "Clearcoat"), coat)
    set_input(("Coat Roughness", "Clearcoat Roughness"), 0.18)
    return material


def _set_principled_input(material, names, value):
    nodes = material.node_tree.nodes if material and material.use_nodes else ()
    principled = nodes.get("Principled BSDF") if nodes else None
    if principled is None:
        return False
    for socket_name in names:
        socket = principled.inputs.get(socket_name)
        if socket is not None:
            socket.default_value = value
            return True
    return False


def _add_bevel(obj, width=0.04, segments=3):
    modifier = obj.modifiers.new("Precision edge softening", "BEVEL")
    modifier.width = width
    modifier.segments = segments
    modifier.limit_method = "ANGLE"
    modifier.angle_limit = math.radians(22.0)
    return modifier


def _rounded_octagonal_radius(theta, radius, corner_blend=0.16):
    """Radius for an octagonal loop with a restrained squircle-like corner."""
    sector = (theta + math.pi / 8.0) % (math.pi / 4.0) - math.pi / 8.0
    octagonal = radius * math.cos(math.pi / 8.0) / max(0.01, math.cos(sector))
    return (octagonal * (1.0 - corner_blend)) + (radius * corner_blend)


def _ring_segment_mesh(name, center, start_deg, end_deg, outer_radius,
                       inner_radius, depth, front_material, side_material,
                       sample_count=72, bevel=0.045):
    """Create a real extruded annular segment with titanium front and graphite walls."""
    start = math.radians(start_deg)
    end = math.radians(end_deg)
    count = max(16, int(sample_count * abs(end - start) / (2.0 * math.pi)))
    thetas = [start + (end - start) * i / float(count - 1) for i in range(count)]

    outer = []
    inner = []
    for theta in thetas:
        outer_r = _rounded_octagonal_radius(theta, outer_radius)
        inner_r = _rounded_octagonal_radius(theta, inner_radius)
        outer.append((center.x + outer_r * math.cos(theta),
                      center.y + outer_r * math.sin(theta)))
        inner.append((center.x + inner_r * math.cos(theta),
                      center.y + inner_r * math.sin(theta)))

    half = depth * 0.5
    vertices = []
    for x, y in outer:
        vertices.append((x, y, half))
    for x, y in inner:
        vertices.append((x, y, half))
    for x, y in outer:
        vertices.append((x, y, -half))
    for x, y in inner:
        vertices.append((x, y, -half))

    outer_front = 0
    inner_front = count
    outer_back = count * 2
    inner_back = count * 3
    faces = []
    material_indices = []

    for i in range(count - 1):
        # Front and rear annular facings.
        faces.append((outer_front + i, outer_front + i + 1,
                      inner_front + i + 1, inner_front + i))
        material_indices.append(0)
        faces.append((outer_back + i, inner_back + i,
                      inner_back + i + 1, outer_back + i + 1))
        material_indices.append(1)

        # The two continuous sidewalls.
        faces.append((outer_front + i, outer_back + i,
                      outer_back + i + 1, outer_front + i + 1))
        material_indices.append(1)
        faces.append((inner_front + i, inner_front + i + 1,
                      inner_back + i + 1, inner_back + i))
        material_indices.append(1)

    # Architectural cut ends; these also make the segment read cleanly at the
    # deliberate narrow gaps.
    faces.append((outer_front, inner_front, inner_back, outer_back))
    material_indices.append(1)
    last = count - 1
    faces.append((outer_front + last, outer_back + last,
                  inner_back + last, inner_front + last))
    material_indices.append(1)

    mesh = bpy.data.meshes.new(_unique_name(bpy.data.meshes, name + " Mesh"))
    mesh.from_pydata(vertices, [], faces)
    mesh.update()
    mesh.materials.append(front_material)
    mesh.materials.append(side_material)
    for polygon, material_index in zip(mesh.polygons, material_indices):
        polygon.material_index = material_index
    obj = bpy.data.objects.new(name, mesh)
    _add_bevel(obj, width=bevel, segments=3)
    return obj


def _extruded_polygon(name, points, z_center, depth, front_material,
                      side_material=None, bevel=0.04):
    """Create a solid planar polygon with explicit front/back and side faces."""
    side_material = side_material or front_material
    half = depth * 0.5
    vertices = [(x, y, z_center + half) for x, y in points]
    vertices.extend((x, y, z_center - half) for x, y in points)
    count = len(points)
    faces = [tuple(range(count)), tuple(range(count * 2 - 1, count - 1, -1))]
    material_indices = [0, 1]
    for i in range(count):
        j = (i + 1) % count
        faces.append((i, j, count + j, count + i))
        material_indices.append(1)
    mesh = bpy.data.meshes.new(_unique_name(bpy.data.meshes, name + " Mesh"))
    mesh.from_pydata(vertices, [], faces)
    mesh.update()
    mesh.materials.append(front_material)
    mesh.materials.append(side_material)
    for polygon, material_index in zip(mesh.polygons, material_indices):
        polygon.material_index = material_index
    obj = bpy.data.objects.new(name, mesh)
    if bevel:
        _add_bevel(obj, width=bevel, segments=3)
    return obj


def _seam_segment(name, center, start_deg, end_deg, outer_radius, inner_radius,
                  material, z=0.275):
    """Low relief teal inlay which follows the inner aperture edge."""
    obj = _ring_segment_mesh(name, center, start_deg, end_deg, outer_radius,
                             inner_radius, 0.028, material, material,
                             sample_count=64, bevel=0.012)
    obj.location.z = z
    return obj


def _new_empty(name, collection, empty_type="PLAIN_AXES"):
    obj = bpy.data.objects.new(name, None)
    obj.empty_display_type = empty_type
    obj.empty_display_size = 0.75
    _link(collection, obj)
    return obj


def _aim_at(obj, target):
    direction = Vector(target) - obj.location
    obj.rotation_euler = direction.to_track_quat("-Z", "Y").to_euler()


def _new_camera(name, collection, location, target, ortho_scale):
    data = bpy.data.cameras.new(_unique_name(bpy.data.cameras, name + " Data"))
    data.type = "ORTHO"
    data.ortho_scale = ortho_scale
    data.lens = 58.0
    obj = bpy.data.objects.new(name, data)
    _link(collection, obj)
    obj.location = location
    _aim_at(obj, target)
    return obj


def _emblem_world_bounds(scene, collection):
    """Return evaluated world-space XY bounds for the generated emblem meshes."""
    points = []
    with bpy.context.temp_override(scene=scene, view_layer=scene.view_layers[0]):
        scene.view_layers[0].update()
        depsgraph = bpy.context.evaluated_depsgraph_get()
        for obj in collection.objects:
            if obj.type != "MESH":
                continue
            evaluated = obj.evaluated_get(depsgraph)
            matrix = evaluated.matrix_world
            points.extend(matrix @ Vector(corner) for corner in evaluated.bound_box)
    if not points:
        return Vector((-3.12, 0.35)), 4.46, 4.46
    min_x = min(point.x for point in points)
    max_x = max(point.x for point in points)
    min_y = min(point.y for point in points)
    max_y = max(point.y for point in points)
    return Vector(((min_x + max_x) * 0.5, (min_y + max_y) * 0.5)), max_x - min_x, max_y - min_y


def _new_area_light(name, collection, location, target, energy, size, color):
    data = bpy.data.lights.new(_unique_name(bpy.data.lights, name + " Data"),
                               type="AREA")
    data.energy = energy
    data.shape = "DISK"
    data.size = size
    data.color = color
    obj = bpy.data.objects.new(name, data)
    _link(collection, obj)
    obj.location = location
    _aim_at(obj, target)
    return obj


def _make_text(name, body, location, size, material, collection, font_path,
               extrude, bevel_depth, character_spacing):
    curve = bpy.data.curves.new(_unique_name(bpy.data.curves, name + " Curve"),
                                type="FONT")
    curve.body = body
    curve.align_x = "LEFT"
    curve.align_y = "CENTER"
    curve.size = size
    curve.extrude = extrude
    curve.bevel_depth = bevel_depth
    curve.bevel_resolution = 3
    curve.resolution_u = 12
    curve.space_character = character_spacing
    if font_path and os.path.exists(font_path):
        curve.font = bpy.data.fonts.load(font_path, check_existing=True)
    obj = bpy.data.objects.new(name, curve)
    _link(collection, obj)
    obj.location = location
    curve.materials.append(material)
    return obj


def _configure_world(scene):
    world = bpy.data.worlds.new(_unique_name(bpy.data.worlds, "Quantix | World"))
    world.use_nodes = True
    background = world.node_tree.nodes.get("Background")
    if background is not None:
        background.inputs["Color"].default_value = (0.005, 0.010, 0.019, 1.0)
        background.inputs["Strength"].default_value = 0.17
    scene.world = world


def _configure_render(scene):
    try:
        scene.render.engine = "CYCLES"
    except Exception:
        # The builder remains usable in a Blender build without Cycles; the
        # parent can select the desired engine before rendering.
        try:
            scene.render.engine = "BLENDER_EEVEE_NEXT"
        except Exception:
            pass
    scene.render.resolution_x = 1600
    scene.render.resolution_y = 900
    scene.render.resolution_percentage = 100
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGBA"
    scene.render.image_settings.color_depth = "8"
    scene.render.filepath = HERO_OUTPUT
    scene.render.film_transparent = False

    if hasattr(scene, "cycles"):
        scene.cycles.device = "CPU"
        scene.cycles.samples = 32
        scene.cycles.use_denoising = True
        scene.cycles.max_bounces = 5
        scene.cycles.diffuse_bounces = 2
        scene.cycles.glossy_bounces = 3
        scene.cycles.transparent_max_bounces = 4
        scene.cycles.adaptive_threshold = 0.035
        scene.cycles.use_adaptive_sampling = True
        if hasattr(scene.cycles, "debug_use_spatial_splits"):
            scene.cycles.debug_use_spatial_splits = True
        scene.render.engine = "CYCLES"
        try:
            scene.render.threads_mode = "FIXED"
            scene.render.threads = 6
        except Exception:
            pass

    try:
        scene.view_settings.view_transform = "AgX"
    except Exception:
        pass
    try:
        scene.view_settings.look = "AgX - Medium High Contrast"
    except Exception:
        try:
            scene.view_settings.look = "Medium High Contrast"
        except Exception:
            pass
    scene.view_settings.exposure = 0.0
    scene.view_settings.gamma = 1.0


def _recalc_scene_mesh_normals(scene):
    """Recalculate outward normals only for meshes linked into this new scene."""
    for obj in scene.objects:
        if obj.type != "MESH" or obj.data is None:
            continue
        mesh_bmesh = bmesh.new()
        try:
            mesh_bmesh.from_mesh(obj.data)
            bmesh.ops.recalc_face_normals(mesh_bmesh, faces=mesh_bmesh.faces)
            mesh_bmesh.to_mesh(obj.data)
            obj.data.update()
        finally:
            mesh_bmesh.free()


def build_scene():
    """Create the Quantix | Atelier scene and return MCP-friendly metadata."""
    global root, emblem_collection, wordmark_collection, studio_collection
    global camera, icon_camera

    scene = _new_scene()
    emblem_collection = _new_collection(scene, "01 | Emblem")
    wordmark_collection = _new_collection(scene, "02 | Wordmark")
    studio_collection = _new_collection(scene, "03 | Studio")

    # Scene-wide custom metadata travels with the .blend and documents the
    # art direction for future export/revision work.
    scene["quantix_concept"] = "Coordinated tender specialists: a structural Q loop with an outward diagonal tail."
    scene["quantix_palette"] = "navy #111d2b; teal #117d76; titanium facings; graphite sidewalls"
    scene["quantix_geometry"] = "Three separated octagonal loop segments; solid Z-depth; narrow inner teal inlay; no decorative noise."
    scene["quantix_render_intent"] = "Hero lockup 1600x900 preview; intended final 3840x2160 with 64/96 Cycles samples."

    root = _new_empty("Quantix | Root", studio_collection, "CUBE")
    root.rotation_euler = tuple(math.radians(value) for value in (14.0, -16.0, -6.0))
    root["design_role"] = "single transform root for editable logo geometry"

    titanium = _material_principled("Quantix | Titanium Facing",
                                    (0.52, 0.61, 0.66), 0.88, 0.25, 0.38)
    graphite = _material_principled("Quantix | Graphite Sidewall",
                                    (0.018, 0.032, 0.047), 0.78, 0.27, 0.18)
    teal = _material_principled("Quantix | Teal Tail",
                                (0.010, 0.245, 0.215), 0.86, 0.2, 0.3)
    teal_inlay = _material_principled("Quantix | Teal Inset Seam",
                                      (0.008, 0.15, 0.145), 0.74, 0.24, 0.22)
    wordmark_material = _material_principled("Quantix | Wordmark Titanium",
                                             (0.62, 0.70, 0.74), 0.82, 0.25, 0.3)
    descriptor_material = _material_principled("Quantix | Descriptor Teal",
                                                (0.026, 0.30, 0.285), 0.66, 0.3, 0.16)
    backplate_material = _material_principled("Quantix | Studio Navy",
                                              (0.002, 0.004, 0.009), 0.05, 0.55, 0.05)
    _set_principled_input(titanium, ("Anisotropic IOR Level", "Anisotropic"), 0.35)
    _set_principled_input(wordmark_material, ("Anisotropic IOR Level", "Anisotropic"), 0.35)

    center = Vector((-3.12, 0.35))
    loop_specs = (
        ("Emblem | Upper Structural Segment", 6.0, 122.0),
        ("Emblem | Left Structural Segment", 126.0, 242.0),
        ("Emblem | Lower Structural Segment", 246.0, 362.0),
    )
    loop_objects = []
    for name, start, end in loop_specs:
        segment = _ring_segment_mesh(name, center, start, end, 2.23, 1.27,
                                     0.46, titanium, graphite, sample_count=88,
                                     bevel=0.052)
        _link(emblem_collection, segment)
        segment.parent = root
        segment["design_role"] = "solid titanium-faced structural Q segment"
        loop_objects.append(segment)

        seam = _seam_segment(name.replace("Structural", "Inset Seam"), center,
                             start + 1.8, end - 1.8, 1.385, 1.315, teal_inlay,
                             z=0.245)
        _link(emblem_collection, seam)
        seam.parent = root
        seam["design_role"] = "restrained teal inset seam following inner aperture"

    # The Q tail has a tapered shoulder inside the aperture and travels out
    # through the lower-right opening as a separate solid teal component.
    tail_start = Vector((center.x + 0.47, center.y - 0.42))
    tail_end = Vector((center.x + 1.90, center.y - 1.77))
    direction = tail_end - tail_start
    direction.normalize()
    perpendicular = Vector((-direction.y, direction.x))
    tail_points = [
        tuple(tail_start + perpendicular * 0.32),
        tuple(tail_end + perpendicular * 0.32),
        tuple(tail_end - perpendicular * 0.32),
        tuple(tail_start - perpendicular * 0.32),
    ]
    tail = _extruded_polygon("Emblem | Q Tail", tail_points, 0.235, 0.40,
                             teal, graphite, bevel=0.045)
    _link(emblem_collection, tail)
    tail.parent = root
    tail["design_role"] = "metallic teal diagonal Q tail from inner aperture"

    # Landscape wordmark.  Keep the curves live and editable; the parent packs
    # the discovered Windows font before saving the native .blend.
    font_path = next((path for path in FONT_CANDIDATES if os.path.exists(path)), "")
    wordmark = _make_text("Wordmark | QUANTIX", "QUANTIX", (-0.47, 0.46, -0.03),
                          1.50, wordmark_material, wordmark_collection,
                          font_path, 0.075, 0.018, 1.08)
    wordmark.parent = root
    wordmark["design_role"] = "primary tender office wordmark"
    descriptor = _make_text("Wordmark | TENDER INTELLIGENCE",
                            "TENDER INTELLIGENCE", (-0.44, -0.58, -0.13),
                            0.29, descriptor_material, wordmark_collection,
                            font_path, 0.028, 0.007, 1.28)
    descriptor.parent = root
    descriptor["design_role"] = "small descriptor beneath primary wordmark"

    converted_text = {"wordmark": False, "descriptor": False}

    # A single navy backplate sits behind the mark.  It is a separate studio
    # object so transparent emblem/icon exports can hide it cleanly.
    backplate_points = [(-6.25, -3.15), (5.70, -3.15), (5.70, 3.15),
                        (-6.25, 3.15)]
    backplate = _extruded_polygon("Studio | Navy Backplate", backplate_points,
                                 -0.35, 0.22, backplate_material,
                                 backplate_material, bevel=0.16)
    _link(studio_collection, backplate)
    backplate.parent = root
    backplate.scale = (4.0, 4.0, 1.0)
    backplate["design_role"] = "separate navy/charcoal studio backplate"

    # Cameras are orthographic for a disciplined engineering lockup; the root
    # tilt exposes depth without compromising the Q silhouette.
    camera = _new_camera("Hero Camera", studio_collection,
                         (-0.45, 0.45, 16.0), (0.0, 0.0, 0.0), 11.7)
    camera.rotation_euler = (0.0, 0.0, 0.0)
    camera.data.lens = 58.0
    camera["design_role"] = "landscape horizontal Quantix lockup"
    emblem_center, emblem_width, emblem_height = _emblem_world_bounds(scene, emblem_collection)
    icon_scale = max(emblem_width, emblem_height) * 1.25
    icon_camera = _new_camera("Icon Camera", studio_collection,
                              (emblem_center.x, emblem_center.y, 15.0),
                              (emblem_center.x, emblem_center.y, 0.0), icon_scale)
    icon_camera.rotation_euler = (0.0, 0.0, 0.0)
    icon_camera.data.lens = 58.0
    icon_camera["design_role"] = "square isolated emblem framing"
    scene.camera = camera

    # Large, quiet softboxes: cool key/fill and a controlled warm edge.
    _new_area_light("Studio | Cool Key Softbox", studio_collection,
                    (-4.6, -3.6, 8.6), (-1.5, 0.2, 0.0), 980.0, 5.2,
                    (0.86, 0.92, 1.0))
    _new_area_light("Studio | Cool Fill Softbox", studio_collection,
                    (4.4, 1.5, 6.2), (0.0, 0.1, 0.0), 520.0, 4.0,
                    (0.32, 0.56, 0.63))
    _new_area_light("Studio | Warm Edge Reflection", studio_collection,
                    (4.9, -1.0, 3.2), (-2.1, 0.2, 0.0), 250.0, 2.0,
                    (1.0, 0.72, 0.48))
    _new_area_light("Studio | Top Strip", studio_collection,
                    (-0.4, 4.5, 5.4), (-1.0, 0.6, 0.0), 330.0, 3.5,
                    (0.42, 0.67, 0.78))

    _configure_world(scene)
    _configure_render(scene)
    _recalc_scene_mesh_normals(scene)

    scene["quantix_font_path"] = font_path or "font unavailable; pack or assign before save"
    scene["quantix_text_conversion"] = "retained editable font curves; pack fonts before saving"
    scene["quantix_hero_camera"] = camera.name
    scene["quantix_icon_camera"] = icon_camera.name
    scene["quantix_output_dir"] = OUTPUT_DIR

    return {
        "scene": scene.name,
        "collections": [emblem_collection.name, wordmark_collection.name,
                        studio_collection.name],
        "root": root.name,
        "hero_camera": camera.name,
        "icon_camera": icon_camera.name,
        "hero_preview_path": HERO_OUTPUT,
        "font_path": font_path,
        "text_converted_to_mesh": converted_text,
        "geometry_assumptions": {
            "front_plane": "XY with thickness along local Z",
            "loop": "three solid annular octagonal/squircle segments, 4 degree gaps",
            "tail": "separate tapered teal prism crossing from inner aperture to lower right",
            "facings": "titanium front, graphite rear/sidewalls, restrained teal inner seams",
            "render": "Cycles CPU, 1600x900 RGBA PNG, 32 preview samples, denoise",
        },
    }


if __name__ == "__main__":
    print(build_scene())
