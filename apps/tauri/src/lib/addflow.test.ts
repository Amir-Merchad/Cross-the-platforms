import { describe, expect, it } from "vitest";
import { addStep, linkChoices, nameLinks, newLink, stepCount } from "./addflow";
import type { Device } from "./devices";
import type { Peer } from "./transfer";

const none = { host: false, peer: false, clip: false };
const peer = (id: string, name: string, kind: Peer["kind"] = "Ipad"): Peer => ({ id, name, kind });
const h = (id: string, name: string | null) => ({ id, name });

describe("addStep", () => {
  it("starts at the Bluetooth step", () => expect(addStep("ipad", none, [])).toBe(0));
  it("moves past Bluetooth when a host appears", () => expect(addStep("ipad", { ...none, host: true }, [])).toBe(1));
  it("a skip moves on without the app seeing anything", () => expect(addStep("ipad", none, [0])).toBe(1));
  it("walks the iPad through three steps", () => {
    expect(addStep("ipad", { host: true, peer: true, clip: false }, [])).toBe(2);
    expect(addStep("ipad", { host: true, peer: true, clip: true }, [])).toBe(3);
  });
  it("the phone has two steps and finishes after files", () => {
    expect(addStep("phone", { host: true, peer: false, clip: false }, [])).toBe(1);
    expect(addStep("phone", { host: true, peer: true, clip: false }, [])).toBe(2);
    expect(stepCount("phone")).toBe(2);
  });
  it("a later step seen does not finish an earlier one", () => expect(addStep("ipad", { ...none, peer: true }, [])).toBe(0));
  it("all skipped is finished", () => expect(addStep("ipad", none, [0, 1, 2])).toBe(3));
});

describe("newLink", () => {
  const before = { hosts: ["old"], peers: ["oldp"] };
  it("links the host and peer that appeared since entry", () =>
    expect(newLink(before, [h("old", "A"), h("new", "B")], [peer("oldp", "A"), peer("p", "B")], {}, {}, "ipad")).toEqual({ host: "new", peer: "p" }));
  it("waits for both", () => expect(newLink(before, [h("new", "B")], [peer("oldp", "A")], {}, {}, "ipad")).toBeNull());
  it("does nothing once the link is written", () => expect(newLink(before, [h("new", "B")], [peer("p", "B")], { new: "p" }, {}, "ipad")).toBeNull());
  it("ignores a new peer of the other kind", () => expect(newLink(before, [h("new", "B")], [peer("p", "B", "Android")], {}, {}, "ipad")).toBeNull());
  it("a phone links to an Android peer", () => expect(newLink(before, [h("new", "B")], [peer("p", "B", "Android")], {}, {}, "phone")).toEqual({ host: "new", peer: "p" }));
  it("an anchored host only needs the new peer", () => expect(newLink(before, [h("old", "A")], [peer("p", "A")], {}, { hostId: "old" }, "ipad")).toEqual({ host: "old", peer: "p" }));
  it("an anchored peer only needs the new host", () => expect(newLink(before, [h("new", "A")], [peer("oldp", "A")], {}, { peerId: "oldp" }, "ipad")).toEqual({ host: "new", peer: "oldp" }));
});

describe("nameLinks", () => {
  it("links identical names", () => expect(nameLinks([h("a", "iPad")], [peer("p", "iPad")], {})).toEqual({ a: "p" }));
  it("skips names that differ", () => expect(nameLinks([h("a", "iPad")], [peer("p", "iPad Air")], {})).toEqual({}));
  it("skips hosts and peers already linked, so nothing is written twice", () => {
    expect(nameLinks([h("a", "iPad")], [peer("p", "iPad")], { a: "p" })).toEqual({});
    expect(nameLinks([h("b", "iPad")], [peer("p", "iPad")], { a: "p" })).toEqual({});
  });
  it("does not guess between two hosts with the same name", () => expect(nameLinks([h("a", "X"), h("b", "X")], [peer("p", "X")], {})).toEqual({}));
  it("still links a unique host beside a duplicated one", () => expect(nameLinks([h("a", "X"), h("b", "X"), h("c", "Y")], [peer("p", "X"), peer("q", "Y")], {})).toEqual({ c: "q" }));
});

describe("linkChoices", () => {
  const dev = (o: Partial<Device>): Device => ({ key: "k", hostId: null, peerId: null, name: "n", kind: null, connected: false, reachable: false, waiting: 0, placement: null, missingScreen: false, colour: "", ...o });
  const hostOnly = dev({ key: "a", hostId: "a" }), peerOnly = dev({ key: "p", peerId: "p" }), both = dev({ key: "b", hostId: "b", peerId: "q" });
  it("a Bluetooth-only device chooses among file-only devices", () => expect(linkChoices(hostOnly, [hostOnly, peerOnly, both])).toEqual([peerOnly]));
  it("a files-only device chooses among Bluetooth-only devices", () => expect(linkChoices(peerOnly, [hostOnly, peerOnly, both])).toEqual([hostOnly]));
  it("a linked device has nothing to choose", () => expect(linkChoices(both, [hostOnly, peerOnly, both])).toEqual([]));
});
