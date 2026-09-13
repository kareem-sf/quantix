"""Quantix identity studies, version 2.

The module creates new Blender scenes only.  It never renders, saves, or
deletes scenes.  All visible marks remain native Blender meshes or editable
FONT curves; the SVG writer evaluates those objects only when requested.
"""

import math
import os
import re

import bpy
from mathutils import Vector


OUTPUT_DIR = r"C:\Users\kareem\.quantix\outputs\brand\quantix-v2-2026-09-09"
FONT_SANS = r"C:\Windows\Fonts\segoeui.ttf"
FONT_SANS_BOLD = r"C:\Windows\Fonts\segoeuib.ttf"
FONT_SERIF = r"C:\Windows\Fonts\georgia.ttf"
CONCEPTS = ("continuum", "junction", "editorial")
CONCEPT_LABELS = {"continuum": "CONTINUUM", "junction": "JUNCTION", "editorial": "EDITORIAL"}


def srgb_hex_to_linear(value):
    """Convert a CSS/sRGB hex color to Blender's linear shader values."""
    value = value.lstrip("#")
    channels = [int(value[index:index + 2], 16) / 255.0 for index in (0, 2, 4)]
    return tuple(channel / 12.92 if channel <= 0.04045 else
                 ((channel + 0.055) / 1.055) ** 2.4 for channel in channels)


def _name(mapping, requested):
    if mapping.get(requested) is None:
        return requested
    i = 1
    while mapping.get("%s.%03d" % (requested, i)) is not None:
        i += 1
    return "%s.%03d" % (requested, i)


def _scene(requested):
    return bpy.data.scenes.new(_name(bpy.data.scenes, requested))


def _collection(scene, requested):
    collection = bpy.data.collections.new(_name(bpy.data.collections, requested))
    scene.collection.children.link(collection)
    return collection


def _link(collection, obj):
    collection.objects.link(obj)
    return obj


def _empty(name, collection, location=(0.0, 0.0, 0.0)):
    obj = bpy.data.objects.new(name, None)
    obj.empty_display_type = "PLAIN_AXES"
    obj.empty_display_size = 0.35
    obj.location = location
    return _link(collection, obj)


def _emission(name, color, strength=1.0):
    material = bpy.data.materials.new(_name(bpy.data.materials, name))
    material.use_nodes = True
    material.diffuse_color = (*color, 1.0)
    nodes = material.node_tree.nodes
    nodes.clear()
    emission = nodes.new("ShaderNodeEmission")
    emission.inputs["Color"].default_value = (*color, 1.0)
    emission.inputs["Strength"].default_value = strength
    output = nodes.new("ShaderNodeOutputMaterial")
    material.node_tree.links.new(emission.outputs[0], output.inputs[0])
    return material


def _principled(name, color, roughness=0.42):
    material = bpy.data.materials.new(_name(bpy.data.materials, name))
    material.use_nodes = True
    material.diffuse_color = (*color, 1.0)
    nodes = material.node_tree.nodes
    nodes.clear()
    shader = nodes.new("ShaderNodeBsdfPrincipled")
    shader.inputs["Base Color"].default_value = (*color, 1.0)
    shader.inputs["Roughness"].default_value = roughness
    shader.inputs["Metallic"].default_value = 0.0
    output = nodes.new("ShaderNodeOutputMaterial")
    material.node_tree.links.new(shader.outputs[0], output.inputs[0])
    return material


def _area_light(name, collection, location, target, energy, size):
    data = bpy.data.lights.new(_name(bpy.data.lights, name + " Data"), "AREA")
    data.energy = energy
    data.shape = "DISK"
    data.size = size
    light = bpy.data.objects.new(_name(bpy.data.objects, name), data)
    light.location = location
    light.rotation_euler = (Vector(target) - light.location).to_track_quat("-Z", "Y").to_euler()
    _link(collection, light)
    return light


def _solid(name, points, material, collection, depth=0.035, z=0.0, bevel=0.0):
    """Create a thin XY polygon with a true front face and editable mesh."""
    area = sum(points[i][0] * points[(i + 1) % len(points)][1] -
               points[(i + 1) % len(points)][0] * points[i][1]
               for i in range(len(points)))
    if area < 0.0:
        points = list(reversed(points))
    half = depth * 0.5
    verts = [(x, y, z + half) for x, y in points] + [(x, y, z - half) for x, y in points]
    n = len(points)
    faces = [tuple(range(n)), tuple(range(2 * n - 1, n - 1, -1))]
    faces += [(i, (i + 1) % n, n + (i + 1) % n, n + i) for i in range(n)]
    mesh = bpy.data.meshes.new(_name(bpy.data.meshes, name + " Mesh"))
    mesh.from_pydata(verts, [], faces)
    mesh.materials.append(material)
    mesh.update()
    obj = bpy.data.objects.new(_name(bpy.data.objects, name), mesh)
    _link(collection, obj)
    if bevel:
        modifier = obj.modifiers.new("Soft shoulder", "BEVEL")
        modifier.width = bevel
        modifier.segments = 3
        modifier.limit_method = "ANGLE"
    return obj


def _text(name, body, location, size, material, collection, font_path, extrude=0.0):
    curve = bpy.data.curves.new(_name(bpy.data.curves, name + " Curve"), "FONT")
    curve.body = body
    curve.align_x = "LEFT"
    curve.align_y = "CENTER"
    curve.size = size
    curve.extrude = extrude
    curve.bevel_depth = 0.0
    curve.bevel_resolution = 0
    curve.resolution_u = 12
    curve.space_character = 1.02
    if font_path and os.path.exists(font_path):
        curve.font = bpy.data.fonts.load(font_path, check_existing=True)
    obj = bpy.data.objects.new(_name(bpy.data.objects, name), curve)
    obj.location = location
    curve.materials.append(material)
    _link(collection, obj)
    return obj


def _continuum_contours():
    """Return the outer CCW and inner CW contours of a single closed Q."""
    outer_radius, inner_radius, half = 1.56, 0.88, 0.31
    axis_angle = math.radians(315.0)
    axis = Vector((math.cos(axis_angle), math.sin(axis_angle)))
    normal = Vector((-axis.y, axis.x))
    outer_start = 315.0 + math.degrees(math.asin(half / outer_radius))
    outer_end = 315.0 - math.degrees(math.asin(half / outer_radius)) + 360.0
    outer = [(outer_radius * math.cos(math.radians(outer_start +
                                                     (outer_end - outer_start) * i / 143.0)),
              outer_radius * math.sin(math.radians(outer_start +
                                                     (outer_end - outer_start) * i / 143.0)))
             for i in range(144)]
    tip = axis * 2.05
    outer.extend((tuple(tip - normal * half), tuple(tip + normal * half)))

    inner_start = 315.0 - math.degrees(math.asin(half / inner_radius))
    inner_end = 315.0 + math.degrees(math.asin(half / inner_radius)) - 360.0
    inner = [(inner_radius * math.cos(math.radians(inner_start -
                                                     (inner_start - inner_end) * i / 143.0)),
              inner_radius * math.sin(math.radians(inner_start -
                                                     (inner_start - inner_end) * i / 143.0)))
             for i in range(144)]
    inner.extend((tuple(axis * 0.45 + normal * half),
                   tuple(axis * 0.45 - normal * half)))
    return outer, inner


def _continuum_curve(name, material, collection, depth=0.035):
    """Native editable 2D Curve with a winding-correct Q counter."""
    curve = bpy.data.curves.new(_name(bpy.data.curves, name + " Curve"), "CURVE")
    curve.dimensions = "2D"
    curve.resolution_u = 12
    curve.fill_mode = "BOTH"
    curve.extrude = depth * 0.5
    outer, inner = _continuum_contours()
    for points in (outer, inner):
        spline = curve.splines.new("POLY")
        spline.points.add(len(points) - 1)
        for point, (x, y) in zip(spline.points, points):
            point.co = (x, y, 0.0, 1.0)
        spline.use_cyclic_u = True
    curve.materials.append(material)
    obj = bpy.data.objects.new(_name(bpy.data.objects, name), curve)
    _link(collection, obj)
    return obj


def _junction_limb_points(sx, sy):
    # A broad tapered petal gives the four-way X a crafted outer silhouette;
    # the two near-center points preserve the open diamond.
    points = [(-0.10, 0.44), (-0.66, 1.05), (-0.95, 1.10),
              (-1.10, 0.95), (-1.05, 0.66), (-0.44, 0.10)]
    points = [(x * sx, y * sy) for x, y in points]
    area = sum(points[i][0] * points[(i + 1) % len(points)][1] -
               points[(i + 1) % len(points)][0] * points[i][1]
               for i in range(len(points)))
    return points if area > 0.0 else list(reversed(points))


def _configure(scene, width, height, transparent, filepath=""):
    try:
        scene.render.engine = "CYCLES"
    except Exception:
        pass
    scene.render.resolution_x = width
    scene.render.resolution_y = height
    scene.render.resolution_percentage = 100
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGBA"
    scene.render.film_transparent = transparent
    scene.render.filepath = filepath
    if hasattr(scene, "cycles"):
        scene.cycles.device = "CPU"
        scene.cycles.samples = 8
        scene.cycles.use_denoising = False
    try:
        scene.view_settings.view_transform = "Standard"
        scene.view_settings.look = "None"
    except Exception:
        pass
    world = bpy.data.worlds.new(_name(bpy.data.worlds, scene.name + " World"))
    world.use_nodes = True
    background = world.node_tree.nodes.get("Background")
    if background:
        background.inputs["Color"].default_value = (*srgb_hex_to_linear("#f6f5f0"), 1.0)
        background.inputs["Strength"].default_value = 1.0
    scene.world = world


def _camera(scene, collection, name, ortho_scale):
    data = bpy.data.cameras.new(_name(bpy.data.cameras, name + " Data"))
    data.type = "ORTHO"
    data.ortho_scale = ortho_scale
    camera = bpy.data.objects.new(_name(bpy.data.objects, name), data)
    camera.location = (0.0, 0.0, 20.0)
    camera.rotation_euler = (0.0, 0.0, 0.0)
    _link(collection, camera)
    scene.camera = camera
    return camera


def _evaluated_bounds(scene, objects):
    points = []
    view_layer = scene.view_layers[0]
    with bpy.context.temp_override(scene=scene, view_layer=view_layer):
        view_layer.update()
        depsgraph = bpy.context.evaluated_depsgraph_get()
        for obj in objects:
            if obj is None:
                continue
            evaluated = obj.evaluated_get(depsgraph)
            if evaluated.type not in {"MESH", "CURVE", "FONT"}:
                continue
            mesh = evaluated.to_mesh()
            try:
                if mesh is not None:
                    points.extend(evaluated.matrix_world @ vertex.co for vertex in mesh.vertices)
            finally:
                evaluated.to_mesh_clear()
    if not points:
        return (-1.0, 1.0, -1.0, 1.0)
    return (min(p.x for p in points), max(p.x for p in points),
            min(p.y for p in points), max(p.y for p in points))


def _fit_group(scene, root, max_width, max_height, center):
    """Scale and translate a root from its actual evaluated artwork bounds."""
    members = [child for child in root.children if child.type in {"MESH", "FONT", "CURVE"}]
    if not members:
        return {"bounds": (-1.0, 1.0, -1.0, 1.0), "scale": 1.0}
    bounds = _evaluated_bounds(scene, members)
    width = max(0.001, bounds[1] - bounds[0])
    height = max(0.001, bounds[3] - bounds[2])
    factor = min(max_width / width, max_height / height)
    root.scale = (root.scale.x * factor, root.scale.y * factor, root.scale.z)
    bounds = _evaluated_bounds(scene, members)
    root.location.x += center[0] - (bounds[0] + bounds[1]) * 0.5
    root.location.y += center[1] - (bounds[2] + bounds[3]) * 0.5
    return {"bounds": _evaluated_bounds(scene, members), "scale": factor}


def _fit_icon_to_wordmark(scene, icon_members, wordmark, ratio=1.3, gap_ratio=0.30):
    """Set the icon's visible height/gap against the actual bold glyph bounds."""
    if not icon_members or wordmark is None:
        return
    icon_bounds = _evaluated_bounds(scene, icon_members)
    word_bounds = _evaluated_bounds(scene, [wordmark])
    icon_height = max(0.001, icon_bounds[3] - icon_bounds[2])
    desired_height = max(0.001, word_bounds[3] - word_bounds[2]) * ratio
    factor = desired_height / icon_height
    for obj in icon_members:
        obj.scale = (obj.scale.x * factor, obj.scale.y * factor, obj.scale.z)
    icon_bounds = _evaluated_bounds(scene, icon_members)
    icon_center_y = (icon_bounds[2] + icon_bounds[3]) * 0.5
    word_center_y = (word_bounds[2] + word_bounds[3]) * 0.5
    shift_y = word_center_y - icon_center_y
    gap = (icon_bounds[1] - icon_bounds[0]) * gap_ratio
    shift_x = word_bounds[0] - gap - icon_bounds[1]
    for obj in icon_members:
        obj.location.x += shift_x
        obj.location.y += shift_y


def _concept_art(scene, concept, collection, material, x, y, scale=1.0,
                 icon_only=False, sculptural=False, prefix=""):
    concept = concept.lower()
    root = _empty("%s%s | Lockup Root" % (prefix, concept.title()), collection, (x, y, 0.0))
    icon_collection = collection
    depth = 0.12 if sculptural else 0.035
    names = []
    members = []
    if concept == "continuum":
        icon = _continuum_curve("%sContinuum | Continuous Q" % prefix, material,
                                icon_collection, depth)
        icon.location = (-2.05 if not icon_only else 0.0, 0.0, 0.0)
        icon.scale = (0.32, 0.32, 1.0)
        icon.parent = root
        icon["design_role"] = "single connected rounded Q ribbon with lower-right taper"
        names.append(icon.name)
        members.append(icon)
        if not icon_only:
            wordmark = _text("%sContinuum | quantix Wordmark" % prefix, "quantix", (-0.66, -0.02, 0.04),
                              0.92, material, collection, FONT_SANS_BOLD, depth * 0.22)
            wordmark.parent = root
            wordmark["design_role"] = "lowercase Segoe UI bold wordmark"
            names.append(wordmark.name)
            members.append(wordmark)
            _fit_icon_to_wordmark(scene, [icon], wordmark)
    elif concept == "junction":
        limbs = []
        for index, (sx, sy) in enumerate(((-1, 1), (1, 1), (-1, -1), (1, -1)), 1):
            limb = _solid("%sJunction | Limb %02d" % (prefix, index),
                          _junction_limb_points(sx, sy), material, icon_collection,
                          depth, bevel=0.05 if sculptural else 0.0)
            limb.parent = root
            limb.scale = (0.32, 0.32, 1.0)
            limb["design_role"] = "mirror-symmetric broad diagonal X limb"
            limbs.append(limb.name)
            members.append(limb)
        aperture = _empty("%sJunction | Open Central Diamond" % prefix, icon_collection)
        aperture.parent = root
        aperture["design_role"] = "unfilled central diamond; preserve at 32px"
        names.extend(limbs)
        names.append(aperture.name)
        if not icon_only:
            wordmark = _text("%sJunction | quantix Wordmark" % prefix, "quantix", (1.16, -0.02, 0.04),
                              0.92, material, collection, FONT_SANS_BOLD, depth * 0.22)
            wordmark.parent = root
            wordmark["design_role"] = "lowercase Segoe UI bold wordmark"
            names.append(wordmark.name)
            members.append(wordmark)
            _fit_icon_to_wordmark(scene, [obj for obj in root.children if obj.type == "MESH"], wordmark)
    elif concept == "editorial":
        if icon_only:
            icon = _text("%sEditorial | Q Icon" % prefix, "Q", (0.0, 0.0, 0.0),
                         2.75, material, collection, FONT_SERIF, depth * 0.22)
            icon.parent = root
            icon["design_role"] = "simple Q icon derived from the Georgia wordmark"
            names.append(icon.name)
        else:
            wordmark = _text("%sEditorial | Quantix Wordmark" % prefix, "Quantix", (-1.28, -0.02, 0.04),
                              1.22, material, collection, FONT_SERIF, depth * 0.22)
            wordmark.parent = root
            wordmark["design_role"] = "Georgia regular editorial wordmark; primary editorial lockup"
            names.append(wordmark.name)
            members.append(wordmark)
    else:
        raise ValueError("Unknown Quantix V2 concept: %s" % concept)
    root.scale = (scale, scale, 1.0)
    root["quantix_v2_concept"] = concept
    root["design_role"] = "shared editable geometry lockup root"
    return {"root": root, "objects": names, "members": members, "concept": concept}


def build_comparison():
    """Create the warm editorial three-row comparison board."""
    scene = _scene("Quantix V2 | Directions")
    board = _collection(scene, "00 | Board")
    dividers = _collection(scene, "01 | Dividers")
    artwork = _collection(scene, "02 | Artwork")
    inverse = _collection(scene, "03 | Inverse Icon Tiles")
    cameras = _collection(scene, "04 | Cameras")
    warm = _emission("Quantix V2 | Warm White", srgb_hex_to_linear("#f6f5f0"))
    black = _emission("Quantix V2 | Near Black", srgb_hex_to_linear("#151817"))
    white = _emission("Quantix V2 | Paper White", srgb_hex_to_linear("#fbfaf6"))
    rule = _emission("Quantix V2 | Hairline", srgb_hex_to_linear("#c8c5bc"))
    _solid("Board | Warm White Field", [(-12, -7.5), (12, -7.5), (12, 7.5), (-12, 7.5)],
           warm, board, 0.03, z=-0.35)
    _text("Board | Heading", "Quantix / Identity studies", (-10.85, 6.45, 0.0),
          0.48, black, board, FONT_SANS, 0.0)
    row_y = (3.35, 0.0, -3.35)
    row_objects = []
    for i, (concept, y) in enumerate(zip(CONCEPTS, row_y), 1):
        label = _text("Board | Label %02d" % i, "%02d / %s" % (i, CONCEPT_LABELS[concept]),
                      (-10.85, y + 0.02, 0.0), 0.30, black, board, FONT_SANS, 0.0)
        label.data.space_character = 1.04
        if i < 3:
            _solid("Board | Divider %02d" % i, [(-10.85, y - 1.64), (10.85, y - 1.64),
                                                 (10.85, y - 1.625), (-10.85, y - 1.625)],
                   rule, dividers, 0.012, z=0.0)
        main = _concept_art(scene, concept, artwork, black, -0.8, y, 1.0,
                            icon_only=False, prefix="Board | ")
        main_fit = _fit_group(scene, main["root"], 7.2, 1.6, (-0.8, y))
        tile_x, tile_w, tile_h = 7.55, 4.95, 2.62
        _solid("Inverse Tile | %s" % CONCEPT_LABELS[concept],
               [(tile_x - tile_w / 2, y - tile_h / 2), (tile_x + tile_w / 2, y - tile_h / 2),
                (tile_x + tile_w / 2, y + tile_h / 2), (tile_x - tile_w / 2, y + tile_h / 2)],
               black, inverse, 0.03, z=-0.15)
        icon = _concept_art(scene, concept, inverse, white, tile_x, y, 1.0,
                            icon_only=True, prefix="Inverse | ")
        icon_fit = _fit_group(scene, icon["root"], 1.65, 1.65, (tile_x, y))
        row_objects.append({"concept": concept, "label": label.name,
                            "main_root": main["root"].name, "icon_root": icon["root"].name,
                            "main_bounds": main_fit["bounds"], "icon_bounds": icon_fit["bounds"]})
    camera = _camera(scene, cameras, "Directions Camera", 24.0)
    _configure(scene, 2400, 1500, False, os.path.join(OUTPUT_DIR, "quantix-v2-directions.png"))
    scene["quantix_v2_kind"] = "editorial comparison board"
    scene["quantix_v2_palette"] = "warm white #f6f5f0; near black #151817; paper white #fbfaf6"
    scene["quantix_v2_camera"] = camera.name
    scene["quantix_v2_output_dir"] = OUTPUT_DIR
    return {"scene": scene.name, "camera": camera.name,
            "collections": [board.name, dividers.name, artwork.name, inverse.name, cameras.name],
            "rows": row_objects, "resolution": (2400, 1500), "output_dir": OUTPUT_DIR}


def build_identity(concept, sculptural=False, width=3200, height=1200):
    """Create a transparent, editable lockup scene for one direction."""
    concept = concept.lower()
    if concept not in CONCEPTS:
        raise ValueError("concept must be one of %s" % (", ".join(CONCEPTS)))
    scene = _scene("Quantix V2 | %s%s" % (concept.title(), " | Sculptural" if sculptural else ""))
    artwork = _collection(scene, "01 | %s Lockup" % CONCEPT_LABELS[concept].title())
    icon_collection = _collection(scene, "02 | Icon")
    camera_collection = _collection(scene, "03 | Cameras")
    material = _emission("Quantix V2 | %s %s" % (CONCEPT_LABELS[concept], "White" if sculptural else "Black"),
                         srgb_hex_to_linear("#fbfaf6" if sculptural else "#151817"))
    lockup = _concept_art(scene, concept, artwork, material, 0.0, 0.0, 1.0,
                          icon_only=False, sculptural=sculptural, prefix="Identity | ")
    lockup_fit = _fit_group(scene, lockup["root"], 9.6, 2.5, (0.0, 0.0))
    # Separate icon copy lets callers hide the wordmark and render a square mark
    # without editing or duplicating the primary lockup's geometry.
    icon_only = _concept_art(scene, concept, icon_collection, material, 0.0, 0.0, 1.0,
                             icon_only=True, sculptural=sculptural, prefix="Icon | ")
    _fit_group(scene, icon_only["root"], 1.65, 1.65, (0.0, 0.0))
    icon_collection.hide_render = True
    camera = _camera(scene, camera_collection, "Lockup Camera", 9.6)
    icon_camera = _camera(scene, camera_collection, "Icon Camera", 4.3)
    icon_camera.location = (0.0, 0.0, 20.0)
    lockup_bounds = lockup_fit["bounds"]
    lockup_width = max(0.001, lockup_bounds[1] - lockup_bounds[0])
    lockup_height = max(0.001, lockup_bounds[3] - lockup_bounds[2])
    aspect = float(width) / float(height)
    camera.data.ortho_scale = max(lockup_width * 1.12, lockup_height * aspect * 1.12)
    icon_bounds = _evaluated_bounds(scene, icon_only["members"])
    icon_camera.data.ortho_scale = max(icon_bounds[1] - icon_bounds[0],
                                       icon_bounds[3] - icon_bounds[2]) * 1.20
    scene.camera = camera
    _configure(scene, width, height, True,
                os.path.join(OUTPUT_DIR, "quantix-%s-lockup.png" % concept))
    scene["quantix_v2_kind"] = "editable transparent identity lockup"
    scene["quantix_v2_concept"] = concept
    scene["quantix_v2_sculptural"] = bool(sculptural)
    scene["quantix_v2_lockup_root"] = lockup["root"].name
    scene["quantix_v2_icon_root"] = icon_only["root"].name
    scene["quantix_v2_icon_camera"] = icon_camera.name
    return {"scene": scene.name, "camera": camera.name, "icon_camera": icon_camera.name,
            "lockup_root": lockup["root"].name, "icon_root": icon_only["root"].name,
            "collections": [artwork.name, icon_collection.name, camera_collection.name],
            "resolution": (width, height), "lockup_bounds": lockup_bounds}


def build_sculptural(concept, width=2048, height=1280):
    """Create a restrained matte studio presentation from the editable lockup."""
    result = build_identity(concept, sculptural=True, width=width, height=height)
    scene = bpy.data.scenes[result["scene"]]
    root = bpy.data.objects[result["lockup_root"]]
    camera = bpy.data.objects[result["camera"]]
    lockup_collection = next(iter(root.users_collection), None)
    studio = _collection(scene, "05 | Sculptural Studio")
    graphite = _principled("Quantix V2 | Sculptural Matte Graphite",
                           srgb_hex_to_linear("#252a27"), roughness=0.46)
    ivory = _principled("Quantix V2 | Sculptural Warm Ivory",
                        srgb_hex_to_linear("#f6f5f0"), roughness=0.82)
    art_members = [child for child in root.children if child.type in {"MESH", "CURVE", "FONT"}]
    for obj in art_members:
        obj.data.materials.clear()
        obj.data.materials.append(graphite)

    root.rotation_euler = (math.radians(10.0), math.radians(-12.0), 0.0)
    background = _solid("Sculptural Studio | Warm Ivory Field",
                        [(-15.0, -15.0), (15.0, -15.0), (15.0, 15.0), (-15.0, 15.0)],
                        ivory, studio, depth=0.04, z=-0.24)
    background.parent = root
    background["design_role"] = "local warm ivory field behind rotated editable lockup"
    art_members = [child for child in root.children if child.type in {"MESH", "CURVE", "FONT"}
                   and child != background]
    bounds = _evaluated_bounds(scene, art_members)
    aspect = float(width) / float(height)
    art_width = max(0.001, bounds[1] - bounds[0])
    art_height = max(0.001, bounds[3] - bounds[2])
    camera.data.ortho_scale = max(art_width * 1.16, art_height * aspect * 1.16)
    lights = [
        _area_light("Sculptural Studio | Key Softbox", studio, (-4.0, -3.0, 8.0),
                    (0.0, 0.0, 0.0), 900.0, 5.0),
        _area_light("Sculptural Studio | Fill Softbox", studio, (4.0, 5.0, 6.0),
                    (0.0, 0.0, 0.0), 450.0, 4.0),
    ]
    scene.camera = camera
    scene.render.film_transparent = False
    scene.render.resolution_x = width
    scene.render.resolution_y = height
    scene.render.resolution_percentage = 100
    scene.render.filepath = os.path.join(OUTPUT_DIR,
                                         "quantix-%s-sculptural.png" % concept.lower())
    try:
        scene.render.engine = "CYCLES"
    except Exception:
        pass
    if hasattr(scene, "cycles"):
        scene.cycles.device = "CPU"
        scene.cycles.samples = 24
        scene.cycles.use_denoising = True
        scene.cycles.max_bounces = 2
        try:
            scene.render.threads_mode = "FIXED"
            scene.render.threads = 6
        except Exception:
            pass
    scene["quantix_v2_kind"] = "editable matte sculptural identity presentation"
    scene["quantix_v2_sculptural_material"] = graphite.name
    scene["quantix_v2_sculptural_background"] = background.name
    scene["quantix_v2_sculptural_lights"] = ", ".join(light.name for light in lights)
    return {"scene": scene.name, "root": root.name, "camera": camera.name,
            "background": background.name, "lights": [light.name for light in lights],
            "collections": [collection.name for collection in (lockup_collection, studio)
                            if collection is not None],
            "resolution": (width, height), "bounds": bounds}


def _svg_path(points, flip_y=True):
    if not points:
        return ""
    coords = [(x, -y if flip_y else y) for x, y in points]
    text = "M %.5f %.5f " % coords[0]
    text += " ".join("L %.5f %.5f" % point for point in coords[1:])
    return text + " Z"


def _svg_compound_path(contours, flip_y=True):
    return " ".join(_svg_path(points, flip_y) for points in contours if points)


def _transform_points(points, scale, offset):
    return [(x * scale + offset[0], y * scale + offset[1]) for x, y in points]


def _fit_svg_groups(groups, word_bounds, initial_scale, initial_offset):
    fitted_groups = [_transform_points(group, initial_scale, initial_offset) for group in groups]
    base = [point for group in fitted_groups for point in group]
    min_x = min(point[0] for point in base)
    max_x = max(point[0] for point in base)
    min_y = min(point[1] for point in base)
    max_y = max(point[1] for point in base)
    icon_height = max(0.001, max_y - min_y)
    word_height = max(0.001, word_bounds[3] - word_bounds[2])
    factor = (word_height * 1.30) / icon_height
    fitted = [[(initial_offset[0] + (x - initial_offset[0]) * factor,
                initial_offset[1] + (y - initial_offset[1]) * factor) for x, y in group]
              for group in fitted_groups]
    base = [point for group in fitted for point in group]
    min_x = min(point[0] for point in base)
    max_x = max(point[0] for point in base)
    min_y = min(point[1] for point in base)
    max_y = max(point[1] for point in base)
    gap = (max_x - min_x) * 0.30
    shift_x = word_bounds[0] - gap - max_x
    shift_y = (word_bounds[2] + word_bounds[3]) * 0.5 - (min_y + max_y) * 0.5
    return [[(x + shift_x, y + shift_y) for x, y in group] for group in fitted]


def _font_outline_paths(scene, objects):
    """Emit welded closed glyph boundaries, avoiding tessellation seams."""
    paths = []
    view_layer = scene.view_layers[0]
    with bpy.context.temp_override(scene=scene, view_layer=view_layer):
        view_layer.update()
        depsgraph = bpy.context.evaluated_depsgraph_get()
        for obj in objects:
            evaluated = obj.evaluated_get(depsgraph)
            mesh = evaluated.to_mesh()
            try:
                if mesh is None:
                    continue
                edge_counts = {}
                canonical = {}

                def welded(point):
                    key = (round(point.x, 7), round(point.y, 7))
                    canonical.setdefault(key, key)
                    return canonical[key]

                for polygon in mesh.polygons:
                    if polygon.normal.z <= 0.5 or len(polygon.vertices) < 3:
                        continue
                    ring = [welded(evaluated.matrix_world @ mesh.vertices[index].co)
                            for index in polygon.vertices]
                    for first, second in zip(ring, ring[1:] + ring[:1]):
                        if first == second:
                            continue
                        edge = tuple(sorted((first, second)))
                        edge_counts[edge] = edge_counts.get(edge, 0) + 1

                boundary = {edge for edge, count in edge_counts.items() if count == 1}
                adjacency = {}
                for first, second in boundary:
                    adjacency.setdefault(first, set()).add(second)
                    adjacency.setdefault(second, set()).add(first)
                loops = []
                while boundary:
                    first, second = boundary.pop()
                    start, current, previous = first, second, first
                    loop = [start, current]
                    while current != start:
                        choices = [candidate for candidate in adjacency.get(current, ())
                                   if tuple(sorted((current, candidate))) in boundary]
                        if not choices:
                            break
                        candidate = choices[0]
                        boundary.remove(tuple(sorted((current, candidate))))
                        previous, current = current, candidate
                        if current != start:
                            loop.append(current)
                        if len(loop) > len(adjacency) + 2:
                            break
                    if current == start and len(loop) >= 3:
                        loops.append(loop)
                if loops:
                    paths.append(_svg_compound_path(loops))
            finally:
                evaluated.to_mesh_clear()
    return paths


def _svg_source_paths(concept):
    """Build a temporary native source scene and return mark/lockup paths."""
    source = _scene("Quantix V2 | SVG Source %s" % CONCEPT_LABELS[concept].title())
    collection = _collection(source, "SVG | %s" % CONCEPT_LABELS[concept].title())
    material = _emission("Quantix V2 | SVG Source Material", srgb_hex_to_linear("#151817"))
    mark_paths = []
    lockup_paths = []
    if concept == "continuum":
        contours = _continuum_contours()
        mark_paths.append(_svg_compound_path(contours))
        wordmark = _text("SVG | Continuum quantix", "quantix", (-0.66, -0.02, 0.0),
                         0.92, material, collection, FONT_SANS_BOLD)
        word_bounds = _evaluated_bounds(source, [wordmark])
        fitted = _fit_svg_groups(contours, word_bounds, 0.32, (-2.05, 0.0))
        lockup_paths.append(_svg_compound_path(fitted))
        lockup_paths.extend(_font_outline_paths(source, [wordmark]))
    elif concept == "junction":
        limbs = []
        for sx, sy in ((-1, 1), (1, 1), (-1, -1), (1, -1)):
            limb = _junction_limb_points(sx, sy)
            mark_paths.append(_svg_path(limb))
            limbs.append(limb)
        wordmark = _text("SVG | Junction quantix", "quantix", (1.16, -0.02, 0.0),
                         0.92, material, collection, FONT_SANS_BOLD)
        word_bounds = _evaluated_bounds(source, [wordmark])
        # Fit the four mirrored limbs as one icon group before placing the gap.
        fitted_limbs = _fit_svg_groups(limbs, word_bounds, 0.32, (0.0, 0.0))
        lockup_paths.extend(_svg_path(limb) for limb in fitted_limbs)
        lockup_paths.extend(_font_outline_paths(source, [wordmark]))
    else:
        wordmark = _text("SVG | Editorial Quantix", "Quantix", (-1.28, -0.02, 0.0),
                         1.22, material, collection, FONT_SERIF)
        q_icon = _text("SVG | Editorial Q", "Q", (0.0, 0.0, 0.0),
                       2.75, material, collection, FONT_SERIF)
        mark_paths = _font_outline_paths(source, [q_icon])
        lockup_paths = _font_outline_paths(source, [wordmark])
    return source.name, mark_paths, lockup_paths


def _svg_viewbox(paths):
    values = []
    for path in paths:
        values.extend(float(value) for value in re.findall(
            r"[-+]?(?:\d+\.?(?:\d*)?|\.\d+)(?:[eE][-+]?\d+)?", path))
    if len(values) < 4:
        return "-1 -1 2 2"
    xs, ys = values[0::2], values[1::2]
    min_x, max_x = min(xs), max(xs)
    min_y, max_y = min(ys), max(ys)
    width = max(0.001, max_x - min_x)
    height = max(0.001, max_y - min_y)
    margin_x, margin_y = width * 0.10, height * 0.10
    return "%.5f %.5f %.5f %.5f" % (min_x - margin_x, min_y - margin_y,
                                     width + margin_x * 2.0, height + margin_y * 2.0)


def _svg_document(title, paths, white=False):
    fill = "#fbfaf6" if white else "#151817"
    viewbox = _svg_viewbox(paths)
    path_markup = "\n".join('  <path d="%s" fill="%s" fill-rule="evenodd"/>' % (path, fill)
                             for path in paths if path)
    return """<svg xmlns="http://www.w3.org/2000/svg" viewBox="%s">
<title>%s</title>
<g>%s</g>
</svg>
""" % (viewbox, title, path_markup)


def export_svgs():
    """Write standalone black/inverse SVG marks and outlined lockups."""
    os.makedirs(OUTPUT_DIR, exist_ok=True)
    paths = []
    source_scenes = []
    for concept in CONCEPTS:
        source_scene, mark_paths, lockup_paths = _svg_source_paths(concept)
        source_scenes.append(source_scene)
        for white, suffix in ((False, "black"), (True, "white")):
            filename = os.path.join(OUTPUT_DIR, "quantix-%s-mark-%s.svg" % (concept, suffix))
            with open(filename, "w", encoding="utf-8", newline="\n") as handle:
                handle.write(_svg_document("Quantix %s mark" % CONCEPT_LABELS[concept].title(),
                                           mark_paths, white))
            paths.append(filename)
            filename = os.path.join(OUTPUT_DIR, "quantix-%s-lockup-%s.svg" % (concept, suffix))
            with open(filename, "w", encoding="utf-8", newline="\n") as handle:
                handle.write(_svg_document("Quantix %s lockup" % CONCEPT_LABELS[concept].title(),
                                           lockup_paths, white))
            paths.append(filename)
    return {"output_dir": OUTPUT_DIR, "files": paths, "svg_source_scenes": source_scenes,
            "note": "SVG text is emitted as evaluated native outlines; original Blender FONT curves remain editable."}


if __name__ == "__main__":
    print(build_comparison())
