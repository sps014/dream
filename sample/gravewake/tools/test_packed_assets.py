from array import array
import json
import math
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]


def words(name, kind):
    result = array(kind)
    result.frombytes((ROOT / 'assets/packed' / name).read_bytes())
    if sys.byteorder != 'little':
        result.byteswap()
    return result


class PackedAssetsTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.manifest = json.loads((ROOT / 'assets/packed/manifest.json').read_text())
        cls.acceleration = words('acceleration.bin', 'f')

    def test_original_procedural_assets_fit_small_geometry_budget(self):
        self.assertTrue(self.manifest['procedural'])
        self.assertEqual(self.manifest['textures'], {})
        for name in self.manifest['files']:
            self.assertTrue((ROOT / 'assets/packed' / name).is_file())
        model = self.manifest['models'][0]
        self.assertGreater(model['triangles'], 300)
        self.assertLess(model['triangles'], 3000)
        self.assertLess(self.manifest['models'][1]['triangles'], 40000)
        self.assertEqual({c['name'] for c in model['clips']}, {'idle', 'walk', 'attack', 'hit', 'death'})
        self.assertEqual(sum(c['count'] for c in model['clips']) * 3, model['frames'])
        self.assertEqual(len(self.manifest['models'][4:]), model['frames'] * 2 + self.manifest['models'][3]['frames'])

    def test_world_hazards_match_visible_geometry_and_expanded_bounds(self):
        level = json.loads((ROOT / 'assets/level.json').read_text())
        self.assertEqual(len(level['hazards']), 6)
        self.assertEqual(len(level['supplies']), 3)
        bounds = level['bounds']
        self.assertGreater(bounds[1] - bounds[0], 100)
        self.assertGreater(bounds[3] - bounds[2], 100)
        world = self.manifest['models'][1]
        vertices = words('vertices.bin', 'f')
        for index, center in enumerate(level['hazards']):
            points = []
            for tri in range(world['triangleFirst'], world['triangleFirst'] + world['triangles']):
                if int(self.acceleration[tri * 4 + 3]) != 70 + index:
                    continue
                for corner in range(3):
                    at = (world['vertexBase'] + int(self.acceleration[tri * 4 + corner])) * 8
                    points.append(vertices[at:at + 3])
            self.assertTrue(points)
            for axis in [0, 2]:
                self.assertAlmostEqual((min(p[axis] for p in points) + max(p[axis] for p in points)) / 2, center[axis], places=4)
            self.assertTrue(bounds[0] < center[0] < bounds[1])
            self.assertTrue(bounds[2] < center[2] < bounds[3])

    def test_native_buffers_preserve_exact_scene_indexes(self):
        self.assertEqual(self.manifest['version'], 6)
        layout = self.manifest['layout']
        self.assertEqual(layout['nodeOffset'], layout['triangleCount'])
        self.assertEqual(layout['catalogOffset'], layout['nodeOffset'] + layout['nodeCount'] * 2)
        self.assertEqual(len(self.acceleration), (layout['catalogOffset'] + len(self.manifest['models']) * 3) * 4)
        self.assertLess(layout['catalogOffset'] + len(self.manifest['models']) * 3, 16777216)
        for name in ('acceleration.bin', 'vertices.bin'):
            self.assertLessEqual((ROOT / 'assets/packed' / name).stat().st_size, 2 * 1024 * 1024 * 1024)

    def test_bvh_ranges_leaf_bounds_and_triangle_indices(self):
        offset = self.manifest['layout']['nodeOffset'] * 4
        for model in self.manifest['models']:
            total = 0
            for at in range(model['root'], model['end']):
                node = self.acceleration[offset + at * 8:offset + at * 8 + 8]
                count = max(0, -int(node[3]))
                escape = at + 1 if count else int(node[3])
                first = int(node[7])
                self.assertGreater(escape, at)
                self.assertLessEqual(escape, model['end'])
                self.assertTrue(all(node[i] <= node[i + 4] for i in range(3)))
                self.assertLessEqual(count, 64)
                if count:
                    total += count
                    self.assertLessEqual(first + count, self.manifest['layout']['triangleCount'])
                    for tri in range(first, first + count):
                        indices = self.acceleration[tri * 4:tri * 4 + 3]
                        self.assertGreaterEqual(min(indices), 0)
                        self.assertLess(max(indices), model['vertices'])
            self.assertEqual(total, model['triangles'])

    def test_town_chunks_partition_static_geometry(self):
        chunks = self.manifest['chunks']
        first = 0
        for bounds, span in zip(chunks[::2], chunks[1::2]):
            self.assertEqual(span[0], first)
            self.assertGreater(span[1], 0)
            self.assertLessEqual(span[1], 8192)
            self.assertGreater(bounds[3], .4)
            first += span[1]
        self.assertEqual(first, self.manifest['models'][1]['triangles'])



if __name__ == '__main__':
    unittest.main()
