// Independent, offline Node WebCrypto fixture check for the future Web client.
// No keys are created, no network or remote control is initiated.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { webcrypto } from "node:crypto";
const { subtle } = webcrypto;
const v = JSON.parse(readFileSync(new URL("../../docs/protocol/v1-fixture.json", import.meta.url), "utf8"));
assert.equal(v.fixture_only, true);
const hex = name => Buffer.from(v[name], "hex");
const text = value => Buffer.from(value, "utf8");
const join = (...parts) => Buffer.concat(parts);
const sha = async b => Buffer.from(await subtle.digest("SHA-256", b));
const verify = async (key, body, signature) => subtle.verify("Ed25519", key, signature, body);
const controller = await subtle.importKey("raw", hex("controller_key"), "Ed25519", false, ["verify"]);
const host = await subtle.importKey("raw", hex("host_key"), "Ed25519", false, ["verify"]);
assert.equal((await sha(hex("controller_key"))).toString("hex"), v.controller_id);
const reg = join(text("Remote EX/register-proof/v1\0"), hex("server_id"), hex("challenge"));
const ctl = join(text("Remote EX/register-controller-only/v1\0"), hex("server_id"), hex("challenge"));
assert(await verify(controller, reg, hex("registration_signature")));
assert(await verify(controller, ctl, hex("controller_registration_signature")));
assert(!(await verify(controller, ctl, hex("registration_signature"))));
const body = join(hex("session_id"), hex("controller_key"), hex("host_key"), hex("controller_nonce"), hex("host_nonce"), Buffer.from([v.requested,v.granted]));
const context = join(text("Remote EX/routed-noise/v1\0"), body, Buffer.from([v.profile, Number(v.relay_only)]));
assert.equal(context.toString("hex"), v.route_context);
const route = role => join(context, Buffer.from([role]), hex("noise_hash"));
assert(await verify(controller,route(1),hex("controller_route_signature")));
assert(await verify(host,route(2),hex("host_route_signature")));
const tampered = Buffer.from(route(1));tampered[context.length-1] ^= 1;
assert(!(await verify(controller,tampered,hex("controller_route_signature"))));
const sdp = async role => join(text("Remote EX/session-transcript/v1\0"), Buffer.from([role]), body, await sha(text(v.offer)), await sha(text(v.answer)));
assert(await verify(controller,await sdp(1),hex("controller_sdp_signature")));
assert(await verify(host,await sdp(2),hex("host_sdp_signature")));
const edited = Buffer.from(await sdp(1));edited[edited.length-1]^=1;
assert(!(await verify(controller,edited,hex("controller_sdp_signature"))));
// Canonical fixture bytes: version=1, session bytes tag=2, seq=1,
// empty release_all message tag=14. This isn't a general Protobuf decoder.
assert.equal(join(Buffer.from([8,1,18,16]),hex("session_id"),Buffer.from([24,1,114,0])).toString("hex"),v.release_all_protobuf);
console.log("PASS offline WebCrypto Ed25519, SHA-256, route/SDP binding, and protobuf fixture bytes");
