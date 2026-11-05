import { describe, expect, it } from "vitest";
import { addStep, finishLine, linkChoices, nameLinks, newHost, newLink, stepCount } from "./addflow";
import type { Device } from "./devices";
import type { Peer } from "./transfer";

const none = { host: false, peer: false, clip: false };
const peer = (id: string, name: string, kind: Peer["kind"] = "Ipad"): Peer => ({ id, name, kind });
const h = (id: string, name: string | null) => ({ id, name });

describe("addStep", () => {
  it("starts at the Bluetooth step", () => expect(addStep("ipad", none, [])).toBe(0));
  it("moves past Bluetooth when a host appears", () => expect(addStep("ipad", { ...none, host: true }, [])).toBe(1));
  it("a skip moves on without the app seeing anything", () => expect(addStep("ipad", none, [0])).toBe(1));
  it("ipad_add_flow_has_two_steps: USB, then Files", () => {
    expect(stepCount("ipad")).toBe(2);
    expect(addStep("ipad", { host: false, peer: false, clip: true }, [])).toBe(1);
    expect(addStep("ipad", { host: true, peer: false, clip: false }, [])).toBe(1);
    expect(addStep("ipad", { host: true, peer: true, clip: false }, [])).toBe(2);
  });
  it("the phone has two steps and finishes after files", () => {
    expect(addStep("phone", { host: true, peer: false, clip: false }, [])).toBe(1);
    expect(addStep("phone", { host: true, peer: true, clip: false }, [])).toBe(2);
    expect(stepCount("phone")).toBe(2);
  });
  it("a later step seen does not finish an earlier one", () => expect(addStep("ipad", { ...none, peer: true }, [])).toBe(0));
  it("all skipped is finished", () => expect(addStep("ipad", none, [0, 1])).toBe(2));
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
  it("never overwrites a good link when an old device reconnects", () =>
    expect(newLink(before, [h("back", "A")], [peer("oldp", "A"), peer("p", "B")], { back: "oldp" }, {}, "ipad")).toBeNull());
  it("skips the reconnecting device for the new one", () =>
    expect(newLink(before, [h("back", "A"), h("new", "B")], [peer("oldp", "A"), peer("p", "B")], { back: "oldp" }, {}, "ipad")).toEqual({ host: "new", peer: "p" }));
});

describe("newHost", () => {
  const before = { hosts: ["old"], peers: ["oldp"] };
  it("is a host that appeared since entry", () => expect(newHost(before, [h("old", "A"), h("new", "B")], {})).toBe("new"));
  it("is not a device linked before the guide opened", () => expect(newHost(before, [h("back", "A")], { back: "oldp" })).toBeUndefined());
  it("stays the same host once the guide has written its link, so the steps do not go back", () =>
    expect(newHost(before, [h("new", "B")], { new: "p" })).toBe("new"));
});

describe("finishLine", () => {
  it("both sides: ready, and where to put it", () =>
    expect(finishLine("ipad", "iPad Air", { host: true, peer: true, clip: true }, false)).toEqual({ title: "iPad Air is ready", body: "Put it next to a screen edge on the Desk." }));
  it("does not ask for a place it already has", () =>
    expect(finishLine("ipad", "iPad Air", { host: true, peer: true, clip: false }, true).body).not.toContain("Put it"));
  it("every step skipped says nothing was added", () => {
    const l = finishLine("ipad", "Your iPad", { host: false, peer: false, clip: false }, false);
    expect(l.title).toBe("Nothing was added");
    expect(l.title + l.body).not.toContain("ready for");
  });
  it("a files-only result never mentions putting it at an edge as the next step", () => {
    const l = finishLine("phone", "Pixel", { host: false, peer: true, clip: false }, false);
    expect(l.title).toBe("Pixel is ready for files and clipboard");
    expect(l.body).not.toContain("Put it");
  });
  it("an iPad's files step leaves the clipboard out", () =>
    expect(finishLine("ipad", "iPad Air", { host: false, peer: true, clip: false }, false).title).toBe("iPad Air is ready for files"));
  it("Bluetooth only says files are still to do", () => {
    const l = finishLine("phone", "Pixel", { host: true, peer: false, clip: false }, false);
    expect(l.title).toBe("Pixel is set up for keyboard and mouse");
    expect(l.body).toContain("Put it next to a screen edge");
    expect(l.body).toContain("files");
  });
  it("the iPad's USB step alone covers keyboard, mouse and clipboard", () => {
    expect(finishLine("ipad", "Your iPad", { host: false, peer: false, clip: true }, false).title).toBe("Your iPad is set up for keyboard, mouse and clipboard");
    expect(finishLine("phone", "Your phone", { host: false, peer: false, clip: true }, false).title).toBe("Nothing was added");
  });
  it("an iPad with files only points at the USB step, not Bluetooth", () =>
    expect(finishLine("ipad", "iPad Air", { host: false, peer: true, clip: false }, false).body).toContain("USB"));
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
  const dev = (o: Partial<Device>): Device => ({ key: "k", hostId: null, peerId: null, name: "n", kind: null, connected: false, reachable: false, waiting: 0, placement: null, missingScreen: false, colour: "", beside: null, ...o });
  const hostOnly = dev({ key: "a", hostId: "a" }), peerOnly = dev({ key: "p", peerId: "p" }), both = dev({ key: "b", hostId: "b", peerId: "q" });
  it("a Bluetooth-only device chooses among file-only devices", () => expect(linkChoices(hostOnly, [hostOnly, peerOnly, both])).toEqual([peerOnly]));
  it("a files-only device chooses among Bluetooth-only devices", () => expect(linkChoices(peerOnly, [hostOnly, peerOnly, both])).toEqual([hostOnly]));
  it("a linked device has nothing to choose", () => expect(linkChoices(both, [hostOnly, peerOnly, both])).toEqual([]));
});
