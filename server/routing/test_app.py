import unittest

import app


def fake_post(routes):
    def post(path, body):
        r = routes.get(body["costing"])
        if r is None:
            raise OSError("no route")
        return {"trip": {"summary": {"length": r[0], "time": r[1]}}}

    return post


A, B = (17.3616, 78.4747), (17.3833, 78.4011)


class RouteTests(unittest.TestCase):
    def test_car_and_scooter(self):
        ans = app.route_answer(fake_post({"auto": (22.64, 2880.0), "motor_scooter": (21.9, 2460.0)}), A, B)
        self.assertEqual(ans, {"km": 22.6, "carMin": 48, "traffic": False, "bikeMin": 41, "bikeMeasured": True})

    def test_scooter_missing_leaves_bike_out(self):
        ans = app.route_answer(fake_post({"auto": (5.0, 600.0)}), A, B)
        self.assertEqual(ans, {"km": 5.0, "carMin": 10, "traffic": False})

    def test_no_car_route(self):
        self.assertIsNone(app.route_answer(fake_post({}), A, B))

    def test_absurd_answers_are_refused(self):
        self.assertIsNone(app.valhalla_summary(fake_post({"auto": (0, 10)}), "auto", A, B))
        self.assertIsNone(app.valhalla_summary(fake_post({"auto": (99999, 10)}), "auto", A, B))
        self.assertIsNone(app.valhalla_summary(fake_post({"auto": (5, 0)}), "auto", A, B))


class InputTests(unittest.TestCase):
    def test_points(self):
        q = {"fromLat": ["17.3"], "fromLon": ["78.4"]}
        self.assertEqual(app.parse_point(q, "fromLat", "fromLon"), (17.3, 78.4))
        for bad in ({}, {"fromLat": ["x"], "fromLon": ["1"]}, {"fromLat": ["91"], "fromLon": ["1"]}, {"fromLat": ["0"], "fromLon": ["0"]}):
            self.assertIsNone(app.parse_point(bad, "fromLat", "fromLon"))

    def test_token(self):
        self.assertTrue(app.token_ok("Bearer abc", "abc"))
        for h, t in (("Bearer abd", "abc"), ("abc", "abc"), (None, "abc"), ("Bearer ", ""), ("Bearer x", "")):
            self.assertFalse(app.token_ok(h, t), (h, t))


if __name__ == "__main__":
    unittest.main()
