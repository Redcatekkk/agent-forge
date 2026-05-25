const express = require("express");
const router = express.Router();

// listUsers returns users visible to the current operator.
router.get("/users", function listUsers(req, res) {
  res.json([]);
});

/**
 * createUser creates a new user account.
 */
router.post("/users", function createUser(req, res) {
  res.status(201).json({ id: "usr_123" });
});
