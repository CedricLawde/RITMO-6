# Blender script: renders the decorative late-70s circuit board seen when the FX panel opens.
# Run:  blender -b --factory-startup --python tools/pcb.py -- assets/pcb.png
# Units = panel design pixels of the board area (628 x 426): x = px - 314, z = 213 - py.
import bpy
import math
import random
import sys

argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
OUT = argv[0] if argv else "pcb.png"
BW, BH = 628, 426
SCALE = 2
K = 1.7  # component size on the board
random.seed(1979)

bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene
scene.render.engine = 'CYCLES'
scene.cycles.device = 'CPU'
scene.cycles.samples = 48
scene.cycles.use_denoising = True
scene.render.resolution_x = BW * SCALE
scene.render.resolution_y = BH * SCALE
scene.render.image_settings.file_format = 'PNG'
try:
    scene.view_settings.view_transform = 'Standard'
except Exception:
    pass


def mat(name, color, metallic=0.0, rough=0.5, bump=0.0, emit=0.0):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    b = nt.nodes.get("Principled BSDF")
    b.inputs["Base Color"].default_value = (*color, 1)
    b.inputs["Metallic"].default_value = metallic
    b.inputs["Roughness"].default_value = rough
    if emit > 0:
        b.inputs["Emission Color"].default_value = (*color, 1)
        b.inputs["Emission Strength"].default_value = emit
    if bump > 0:
        tc = nt.nodes.new("ShaderNodeTexCoord")
        nz = nt.nodes.new("ShaderNodeTexNoise")
        nz.inputs["Scale"].default_value = 0.08
        nz.inputs["Detail"].default_value = 10.0
        bp = nt.nodes.new("ShaderNodeBump")
        bp.inputs["Strength"].default_value = bump
        nt.links.new(tc.outputs["Object"], nz.inputs["Vector"])
        nt.links.new(nz.outputs["Fac"], bp.inputs["Height"])
        nt.links.new(bp.outputs["Normal"], b.inputs["Normal"])
    return m


BOARD = mat("board", (0.20, 0.085, 0.035), rough=0.55, bump=0.15)
SILK = mat("silk", (0.85, 0.82, 0.72), rough=0.6)
SOLDER = mat("solder", (0.75, 0.75, 0.72), metallic=1.0, rough=0.3)
COPPER = mat("copper", (0.55, 0.28, 0.12), metallic=0.7, rough=0.35)
RES = mat("resistor", (0.78, 0.62, 0.40), rough=0.45)
BANDS = [mat("b%d" % i, c, rough=0.4) for i, c in enumerate([
    (0.02, 0.02, 0.02), (0.30, 0.12, 0.03), (0.75, 0.05, 0.03), (0.85, 0.35, 0.02),
    (0.85, 0.75, 0.05), (0.08, 0.40, 0.10), (0.05, 0.15, 0.60), (0.75, 0.62, 0.15)])]
GREENCAP = mat("greencap", (0.07, 0.30, 0.12), rough=0.3)
ORANGECAP = mat("orangecap", (0.75, 0.35, 0.05), rough=0.3)
BROWNCAP = mat("browncap", (0.28, 0.08, 0.05), rough=0.35)
ELKO = mat("elko", (0.30, 0.32, 0.70), rough=0.35)
ELKO_TOP = mat("elkotop", (0.75, 0.76, 0.78), metallic=1.0, rough=0.3)
BLACK = mat("black", (0.02, 0.02, 0.02), rough=0.4)
WHITE = mat("white", (0.88, 0.88, 0.84), rough=0.5)
XFORMER = mat("xformer", (0.70, 0.66, 0.55), rough=0.5)
LED_MAT = mat("led", (0.5, 0.03, 0.02), rough=0.15)
WIRES = [mat("w%d" % i, c, rough=0.45) for i, c in enumerate([
    (0.75, 0.05, 0.03), (0.85, 0.65, 0.05), (0.05, 0.20, 0.70), (0.08, 0.45, 0.12),
    (0.85, 0.85, 0.82), (0.45, 0.45, 0.45), (0.85, 0.35, 0.03), (0.40, 0.08, 0.45)])]


def loc(px, py, y):
    return (px - BW / 2, y, BH / 2 - py)


def cube(px, py, w, h, depth, material, bevel=0.0, y=0.0, rot=0.0, k=K):
    bpy.ops.mesh.primitive_cube_add(size=1.0)
    o = bpy.context.active_object
    o.scale = (w * k, depth, h * k)
    o.location = loc(px, py, y - depth / 2)
    o.rotation_euler = (0, rot, 0)
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    if bevel > 0:
        b = o.modifiers.new("bv", "BEVEL")
        b.width = bevel
        b.segments = 3
        b.limit_method = 'NONE'
    o.data.materials.append(material)
    return o


def cyl_up(px, py, r, height, material, y=0.0):
    """Cylinder standing on the board (axis toward the camera)."""
    bpy.ops.mesh.primitive_cylinder_add(radius=r * K, depth=height, vertices=28)
    o = bpy.context.active_object
    o.rotation_euler = (math.radians(90), 0, 0)
    o.location = loc(px, py, y - height / 2)
    o.data.materials.append(material)
    bpy.ops.object.shade_smooth()
    return o


def cyl_flat(px, py, r, length, material, horizontal=True, y=-2.0, k=K):
    """Cylinder lying on the board."""
    bpy.ops.mesh.primitive_cylinder_add(radius=r * k, depth=length * k, vertices=20)
    o = bpy.context.active_object
    o.rotation_euler = (0, math.radians(90), 0) if horizontal else (0, 0, 0)
    o.location = loc(px, py, y)
    o.data.materials.append(material)
    bpy.ops.object.shade_smooth()
    return o


def text(px, py, body, size, material=SILK):
    bpy.ops.object.text_add()
    o = bpy.context.active_object
    o.data.body = body
    o.data.size = size * K
    o.data.align_x = 'CENTER'
    o.data.align_y = 'CENTER'
    o.data.extrude = 0.05
    o.rotation_euler = (math.radians(90), 0, 0)
    o.location = loc(px, py, -0.15)
    o.data.materials.append(material)
    return o


occupied = set()
CELL = 14


def take(px, py, w, h):
    w, h = w * K, h * K
    cells = [(cx, cy) for cx in range(int((px - w / 2) // CELL), int((px + w / 2) // CELL) + 1)
             for cy in range(int((py - h / 2) // CELL), int((py + h / 2) // CELL) + 1)]
    if any(c in occupied for c in cells):
        return False
    occupied.update(cells)
    return True


# board, mounting screws and edge connectors
cube(BW / 2, BH / 2, BW, BH, 3.0, BOARD, y=0.0, k=1.0)
for (px, py) in ((14, 14), (BW - 14, 14), (14, BH - 14), (BW - 14, BH - 14)):
    cyl_up(px, py, 6.0, 2.0, SOLDER, y=-3.0)
    cube(px, py, 7.0, 1.0, 0.5, BLACK, y=-5.1)
    take(px, py, 24, 24)

# faint copper traces between areas
for _ in range(70):
    x, y = random.uniform(30, BW - 30), random.uniform(30, BH - 30)
    if random.random() < 0.5:
        cube(x, y, random.uniform(20, 90), 1.4, 0.2, COPPER, y=-3.0, k=1.0)
    else:
        cube(x, y, 1.4, random.uniform(20, 70), 0.2, COPPER, y=-3.0, k=1.0)

# a few big parts first: brown film caps, transformers, a bank of electrolytics
for (px, py, r) in ((150, 110, 15), (186, 110, 15), (330, 120, 14), (366, 120, 14), (470, 90, 13),
                    (120, 300, 13), (250, 300, 14), (520, 260, 14)):
    if take(px, py, r * 2 + 4, r * 2 + 4):
        cyl_up(px, py, r, 16, BROWNCAP, y=-3.0)
        cyl_up(px, py, r * 0.92, 0.5, mat("ct%d" % px, (0.34, 0.11, 0.06), rough=0.25), y=-19.0)
for (px, py) in ((400, 250), (450, 250)):
    if take(px, py, 40, 46):
        cube(px, py, 34, 40, 18, XFORMER, bevel=1.5, y=-3.0)
        cube(px, py, 22, 40.5, 14, COPPER, y=-5.0)
for _ in range(16):
    px, py, r = random.uniform(40, BW - 40), random.uniform(40, BH - 40), random.choice((5, 6, 7, 8))
    if take(px, py, r * 2 + 4, r * 2 + 4):
        cyl_up(px, py, r, 12, ELKO, y=-3.0)
        cyl_up(px, py, r * 0.85, 0.6, ELKO_TOP, y=-15.0)
        cube(px, py, r * 1.2, 0.6, 0.3, BLACK, y=-15.7)

# trimmers
for _ in range(7):
    px, py = random.uniform(40, BW - 40), random.uniform(40, BH - 40)
    if take(px, py, 12, 12):
        cube(px, py, 9, 9, 3.0, WHITE, bevel=0.8, y=-3.0)
        cyl_up(px, py, 3.0, 1.0, BLACK, y=-6.0)
        cube(px, py, 5, 0.9, 0.5, WHITE, y=-7.0, rot=random.uniform(0, 3))

# transistors (TO-92 from above), resistors, small caps
for _ in range(1400):
    kind = random.random()
    px, py = random.uniform(30, BW - 30), random.uniform(30, BH - 30)
    if kind < 0.15:
        if take(px, py, 8, 8):
            cyl_up(px, py, 2.8, 5.0, BLACK, y=-3.0)
            cube(px, py + 1.6 * K, 5.6, 2.0, 5.0, BLACK, y=-3.0)
    elif kind < 0.6:
        horizontal = random.random() < 0.6
        w, h = (16, 6) if horizontal else (6, 16)
        if take(px, py, w, h):
            cyl_flat(px, py, 1.7, 8.0, RES, horizontal)
            for i, b in enumerate(random.sample(BANDS, 3)):
                off = (-2.2 + i * 1.6) * K
                bx, by = (px + off, py) if horizontal else (px, py + off)
                cyl_flat(bx, by, 1.8, 0.7, b, horizontal)
            for s in (-7.0, 7.0):
                lx, ly = (px + s * K, py) if horizontal else (px, py + s * K)
                cyl_up(lx, ly, 1.1, 0.6, SOLDER, y=-3.0)
    elif kind < 0.85:
        if take(px, py, 9, 7):
            m = random.choice((GREENCAP, GREENCAP, ORANGECAP))
            cube(px, py, 7, 4.5, 4.0, m, bevel=1.6, y=-3.0)
    else:
        if take(px, py, 8, 8):
            cyl_up(px, py, 3.0, 3.0, random.choice((ORANGECAP, GREENCAP)), y=-3.0)

# the sequencer LED (the app draws its glow on top)
cyl_up(560, 64, 3.2, 4.0, LED_MAT, y=-3.0)
text(560, 76, "LD1", 5.0)

# wire looms along the left and bottom edges
for i, w in enumerate(WIRES):
    cyl_flat(8 + i * 2.6, BH / 2, 1.2, BH - 40, w, horizontal=False, y=-4.0 - (i % 3) * 1.1, k=1.0)
    cyl_flat(BW / 2, BH - 6 - i * 2.0, 1.1, BW - 60, w, horizontal=True, y=-4.0 - (i % 2) * 1.1, k=1.0)

# silk-screen labels
text(BW / 2, 28, "RT-6  FX MODULE   SB-079", 9.0)
for (px, py, s) in ((70, 60, "PH"), (220, 60, "DS"), (380, 60, "DL"), (520, 130, "SP"),
                    (40, 200, "IN"), (BW - 40, 200, "OUT"), (300, 400, "GND"), (420, 400, "+15V"),
                    (160, 380, "-15V"), (560, 380, "CLK")):
    text(px, py, s, 7.0)
for i in range(26):
    text(random.uniform(40, BW - 40), random.uniform(40, BH - 40),
         random.choice(("R", "C", "Q", "VR")) + str(random.randint(1, 64)), 4.0)

# light and camera
bpy.ops.object.light_add(type='SUN')
sun = bpy.context.active_object
sun.data.energy = 3.2
sun.data.angle = math.radians(12)
sun.rotation_euler = (math.radians(65), math.radians(-25), math.radians(-35))
world = bpy.data.worlds.new("world")
scene.world = world
world.use_nodes = True
world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.45, 0.42, 0.36, 1)
world.node_tree.nodes["Background"].inputs["Strength"].default_value = 0.35
bpy.ops.object.camera_add(location=(0, -1500, 0), rotation=(math.radians(90), 0, 0))
cam = bpy.context.active_object
cam.data.type = 'ORTHO'
cam.data.ortho_scale = BW
cam.data.clip_end = 5000
scene.camera = cam
scene.render.filepath = OUT
bpy.ops.render.render(write_still=True)
print("rendered", OUT)
