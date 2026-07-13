# Peerlink

A cross-platform peer-to-peer remote desktop system: one machine shares its screen and accepts remote input; another views the stream and sends input.

## Language

### Roles

**Host**:
The peer that captures its screen, encodes and sends video, and injects received input into the OS.
_Avoid_: Server, sharer, broadcaster

**Client**:
The peer that receives and displays the remote screen and sends local mouse/keyboard events to the host.
_Avoid_: Viewer (as the role name), receiver

### Pipeline stages

**Capture**:
Obtaining screen frames from the operating system on the host.

**Encode**:
Compressing captured frames into a bitstream for transport.

**Transport**:
Carrying encoded media and control/input messages between peers over the network.

**Decode**:
Reconstructing frames from the received bitstream on the client.

**Render**:
Displaying decoded frames in the client's UI.

**Input**:
Mouse and keyboard events from the client, delivered to the host and injected as synthetic OS input.
_Avoid_: Control (overloaded with connection control)

### Connectivity

**Signaling**:
A side channel used only to exchange connection metadata before (or while) establishing the media path; not the video data path.
_Avoid_: Matchmaking server (unless used loosely in explanation)

**STUN**:
A service a peer queries to learn its public-facing address as seen through NAT.

**TURN**:
A relay used when a direct peer-to-peer path cannot be established; media may flow through the relay.

**ICE**:
The process of gathering address candidates (host, server-reflexive via STUN, relay via TURN), checking connectivity, and selecting a working path.

**LAN direct-connect**:
Establishing transport by explicitly addressing a peer on the same local network (e.g. IP and port), without signaling or NAT traversal.
