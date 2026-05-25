const express = require("express");
const app = express();

// This route should be ignored because openapi.yaml is the source of truth.
app.get("/debug", function debugRoute(req, res) {
  res.json({ ok: true });
});
