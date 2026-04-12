import { describe, expect, it } from "vitest";
import express from "express";
import request from "supertest";
import { SHARED_SECRET_HEADER, createSharedSecretMiddleware } from "./auth.js";

function buildApp(secret: string) {
  const app = express();
  app.use("/api/copilotkit", createSharedSecretMiddleware(secret));
  app.post("/api/copilotkit", (_req, res) => {
    res.json({ reached: true });
  });
  return app;
}

describe("createSharedSecretMiddleware", () => {
  const secret = "correct-horse-battery-staple";

  it("rejects requests missing the shared-secret header with 401", async () => {
    const app = buildApp(secret);

    const response = await request(app).post("/api/copilotkit").send({});

    expect(response.status).toBe(401);
    expect(response.body).toMatchObject({
      error: "missing_or_invalid_shared_secret",
    });
    expect(response.body.hint).toContain(SHARED_SECRET_HEADER);
  });

  it("rejects requests with a wrong shared-secret header", async () => {
    const app = buildApp(secret);

    const response = await request(app)
      .post("/api/copilotkit")
      .set(SHARED_SECRET_HEADER, "not-the-secret")
      .send({});

    expect(response.status).toBe(401);
    expect(response.body.reached).toBeUndefined();
  });

  it("allows requests with the correct shared-secret header", async () => {
    const app = buildApp(secret);

    const response = await request(app)
      .post("/api/copilotkit")
      .set(SHARED_SECRET_HEADER, secret)
      .send({});

    expect(response.status).toBe(200);
    expect(response.body).toEqual({ reached: true });
  });

  it("passes CORS preflight OPTIONS requests through without a secret", async () => {
    // Preflight must bypass the header check because browsers do not send
    // custom headers on OPTIONS. Without this exemption, every request from
    // ui_web would fail at the preflight stage.
    const app = buildApp(secret);
    app.options("/api/copilotkit", (_req, res) => {
      res.status(204).send();
    });

    const response = await request(app).options("/api/copilotkit");

    expect(response.status).toBe(204);
  });

  it("rejects requests whose secret differs only in length", async () => {
    const app = buildApp(secret);

    const response = await request(app)
      .post("/api/copilotkit")
      .set(SHARED_SECRET_HEADER, `${secret}-extra`)
      .send({});

    expect(response.status).toBe(401);
  });

  it("refuses to construct with an empty secret", () => {
    expect(() => createSharedSecretMiddleware("")).toThrowError(
      /empty secret.*localhost hardening/,
    );
  });
});
