# Blender script: renders the Ritmo-6 chassis (front view, orthographic) to a PNG.
# Run:  blender -b --factory-startup --python tools/chassis.py -- assets/chassis.png
# Scene units match the panel's design pixels (760 x 490): x = px - 380, z = 245 - py.
import bpy
import math
import sys

argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
OUT = argv[0] if argv else "chassis.png"
SCALE = 2  # render at 2x the design size

bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene
scene.render.engine = 'CYCLES'
scene.cycles.device = 'CPU'
scene.cycles.samples = 64
scene.cycles.use_denoising = True
scene.render.resolution_x = 760 * SCALE
scene.render.resolution_y = 490 * SCALE
scene.render.resolution_percentage = 100
scene.render.film_transparent = True
scene.render.image_settings.file_format = 'PNG'
scene.render.image_settings.color_mode = 'RGBA'
try:
    scene.view_settings.view_transform = 'Standard'
except Exception:
    pass


def mat(name, color, metallic=0.0, rough=0.5, bump=None, wood=False, brushed=False):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    bsdf = nt.nodes.get("Principled BSDF")
    bsdf.inputs["Base Color"].default_value = (*color, 1.0)
    bsdf.inputs["Metallic"].default_value = metallic
    bsdf.inputs["Roughness"].default_value = rough
    if wood:
        tc = nt.nodes.new("ShaderNodeTexCoord")
        mp = nt.nodes.new("ShaderNodeMapping")
        mp.inputs["Scale"].default_value = (0.02, 0.02, 0.0018)
        wave = nt.nodes.new("ShaderNodeTexWave")
        wave.inputs["Scale"].default_value = 2.0
        wave.inputs["Distortion"].default_value = 7.0
        wave.inputs["Detail"].default_value = 3.0
        ramp = nt.nodes.new("ShaderNodeValToRGB")
        ramp.color_ramp.elements[0].color = (0.16, 0.075, 0.025, 1)
        ramp.color_ramp.elements[1].color = (0.36, 0.19, 0.07, 1)
        nt.links.new(tc.outputs["Object"], mp.inputs["Vector"])
        nt.links.new(mp.outputs["Vector"], wave.inputs["Vector"])
        nt.links.new(wave.outputs["Fac"], ramp.inputs["Fac"])
        nt.links.new(ramp.outputs["Color"], bsdf.inputs["Base Color"])
    if bump or brushed:
        tc = nt.nodes.new("ShaderNodeTexCoord")
        mp = nt.nodes.new("ShaderNodeMapping")
        mp.inputs["Scale"].default_value = (0.02, 1.0, 1.2) if brushed else (0.3, 0.3, 0.3)
        noise = nt.nodes.new("ShaderNodeTexNoise")
        noise.inputs["Scale"].default_value = 40.0 if brushed else 8.0
        noise.inputs["Detail"].default_value = 8.0
        bp = nt.nodes.new("ShaderNodeBump")
        bp.inputs["Strength"].default_value = 0.08 if brushed else (bump or 0.3)
        nt.links.new(tc.outputs["Object"], mp.inputs["Vector"])
        nt.links.new(mp.outputs["Vector"], noise.inputs["Vector"])
        nt.links.new(noise.outputs["Fac"], bp.inputs["Height"])
        nt.links.new(bp.outputs["Normal"], bsdf.inputs["Normal"])
    return m


LEATHER = mat("leather", (0.008, 0.008, 0.009), rough=0.75, bump=0.35)
PANEL = mat("panel", (0.018, 0.027, 0.04), metallic=0.25, rough=0.5, brushed=True)
CHROME = mat("chrome", (0.85, 0.86, 0.88), metallic=1.0, rough=0.14)
DARKCHROME = mat("darkchrome", (0.22, 0.23, 0.25), metallic=1.0, rough=0.25)
WOOD = mat("wood", (0.5, 0.3, 0.12), rough=0.45, wood=True)
RUBBER = mat("rubber", (0.03, 0.03, 0.03), rough=0.8)
BAY = mat("bay", (0.004, 0.004, 0.005), rough=0.85)


def box(name, x0, y0, x1, y1, depth, front, bevel, material, segments=5):
    w, h = x1 - x0, y1 - y0
    bpy.ops.mesh.primitive_cube_add(size=1.0)
    o = bpy.context.active_object
    o.name = name
    o.scale = (w, depth, h)
    o.location = ((x0 + x1) / 2 - 380, front + depth / 2, 245 - (y0 + y1) / 2)
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    if bevel > 0:
        b = o.modifiers.new("bevel", "BEVEL")
        b.width = bevel
        b.segments = segments
        b.limit_method = 'NONE'
    o.data.materials.append(material)
    bpy.ops.object.shade_smooth()
    return o


def screw(px, py, r=3.2):
    bpy.ops.mesh.primitive_cylinder_add(radius=r, depth=2.0, vertices=24)
    o = bpy.context.active_object
    o.rotation_euler = (math.radians(90), 0, 0)
    o.location = (px - 380, -4.5, 245 - py)
    o.data.materials.append(CHROME)
    bpy.ops.object.shade_smooth()
    # slot
    s = box("slot", px - r * 0.8, py - 0.45, px + r * 0.8, py + 0.45, 0.6, -5.6, 0, BAY)
    s.rotation_euler = (0, math.radians(35), 0)


# 60s/70s moulded plastic: glossy ivory shell, charcoal faceplate, orange side panel.
IVORY = mat("ivory", (0.78, 0.70, 0.55), rough=0.22, bump=0.02)
IVORY_DARK = mat("ivory_dark", (0.55, 0.48, 0.36), rough=0.3)
CHARCOAL = mat("charcoal", (0.022, 0.024, 0.028), rough=0.55, bump=0.04)
ORANGE = mat("orange", (0.80, 0.25, 0.03), rough=0.2, bump=0.02)
BROWN = mat("brown", (0.05, 0.03, 0.02), rough=0.35)

# shell: one rounded moulding with a raised lip around the face
box("case", 8, 18, 752, 480, 90, 0, 26, IVORY, 8)
box("lip", 26, 30, 742, 470, 6, -2.0, 10, IVORY_DARK, 6)
# moulded side grips (grooves)
for x in (14, 20):
    box("groove", x, 120, x + 3, 380, 2, -0.6, 1.2, IVORY_DARK, 3)
    box("groove", 760 - x - 3, 120, 760 - x, 380, 2, -0.6, 1.2, IVORY_DARK, 3)
# plastic carrying handle, moulded with the shell
box("handle_l", 290, 2, 316, 26, 20, -8, 8, IVORY, 6)
box("handle_r", 444, 2, 470, 26, 20, -8, 8, IVORY, 6)
box("handle", 300, 3, 460, 15, 16, -6, 6, IVORY, 6)
# round rubber feet
for (x0, x1) in ((112, 158), (602, 648)):
    box("foot", x0, 474, x1, 489, 40, 6, 7, BROWN, 6)
# charcoal faceplate and orange side panel, with soft rounded edges
box("plate", 34, 38, 582, 464, 4, -4.0, 4, CHARCOAL, 5)
box("bay", 584, 34, 734, 468, 4, -4.0, 5, ORANGE, 5)
box("bay_inner", 589, 254, 729, 462, 2, -4.6, 3, CHARCOAL, 4)
# moulded plastic studs instead of screws
for (px, py) in ((42, 46), (574, 46), (42, 456), (574, 456), (592, 40), (726, 40), (592, 462), (726, 462)):
    bpy.ops.mesh.primitive_uv_sphere_add(radius=3.2, segments=24, ring_count=12)
    o = bpy.context.active_object
    o.scale = (1, 0.45, 1)
    o.location = (px - 380, -4.6, 245 - py)
    o.data.materials.append(IVORY_DARK)
    bpy.ops.object.shade_smooth()

# light and camera
bpy.ops.object.light_add(type='SUN')
sun = bpy.context.active_object
sun.data.energy = 3.0
sun.data.angle = math.radians(14)
sun.rotation_euler = (math.radians(70), math.radians(-25), math.radians(-30))
bpy.ops.object.light_add(type='SUN')
fill = bpy.context.active_object
fill.data.energy = 1.2
fill.data.angle = math.radians(40)
fill.rotation_euler = (math.radians(95), math.radians(20), math.radians(20))
world = bpy.data.worlds.new("world")
scene.world = world
world.use_nodes = True
world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.42, 0.40, 0.36, 1)
world.node_tree.nodes["Background"].inputs["Strength"].default_value = 0.25

bpy.ops.object.camera_add(location=(0, -1500, 0), rotation=(math.radians(90), 0, 0))
cam = bpy.context.active_object
cam.data.type = 'ORTHO'
cam.data.ortho_scale = 760
cam.data.clip_end = 5000
scene.camera = cam

scene.render.filepath = OUT
bpy.ops.render.render(write_still=True)
print("rendered", OUT)
