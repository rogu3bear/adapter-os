//! Protocol types for agent communication
//!
//! This module defines the wire format for orchestrator ↔ agent communication
//! over Unix Domain Sockets using HTTP/1.1 as the transport layer.

pub mod handshake;
pub mod messages;

pub use handshake::{AgentCapabilities, HandshakeRequest, HandshakeResponse, HandshakeStatus};
pub use messages::{
    AgentRequest, AgentResponse, AgentState, AgentStatus, FileModification, ModificationType,
    TaskAssignment, TaskConstraints, TaskProgress, TaskProposal, TaskScope,
};
