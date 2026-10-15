//! Descriptors: what the registry knows of one command or query (API-01).

use core_txn::{ChangeEvent, Origin};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ApiError, Signature, Version};

/// Whether a call may change the model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Changes the model, and is recorded in its history with its origin.
    Command,
    /// Reads the model and never changes it.
    Query,
}

/// What a command or query returns: its result, and the change a command made, which transports
/// send to every client as a notification.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome<T = Value> {
    /// The call's output.
    pub result: T,
    /// What a command changed; always `None` for a query.
    pub event: Option<ChangeEvent>,
}

type CommandFn<Ctx> = dyn Fn(&mut Ctx, Value, &Origin) -> Result<Outcome, ApiError>;
type QueryFn<Ctx> = dyn Fn(&Ctx, Value) -> Result<Value, ApiError>;

/// A query only borrows the context, so the compiler guarantees it changes nothing.
enum Handler<Ctx> {
    Command(Box<CommandFn<Ctx>>),
    Query(Box<QueryFn<Ctx>>),
}

/// A command or query: its stable name, its version and its handler, which runs on a context
/// `Ctx` such as a document. The handler is written with typed input and output; the descriptor
/// decodes the JSON parameters into the input and encodes the output, so every transport shares
/// one decoder.
pub struct Descriptor<Ctx> {
    name: &'static str,
    version: Version,
    summary: &'static str,
    signature: Signature,
    handler: Handler<Ctx>,
}

impl<Ctx> Descriptor<Ctx> {
    /// A command named `name`, described in one sentence by `summary`, whose parameters and result
    /// `signature` describes, that runs `handler` with the caller's origin.
    pub fn command<I, O>(
        name: &'static str,
        version: Version,
        summary: &'static str,
        signature: Signature,
        handler: impl Fn(&mut Ctx, I, &Origin) -> Result<Outcome<O>, ApiError> + 'static,
    ) -> Self
    where
        Ctx: 'static,
        I: DeserializeOwned + 'static,
        O: Serialize + 'static,
    {
        let erased = move |ctx: &mut Ctx, params: Value, origin: &Origin| {
            let outcome = handler(ctx, decode(params)?, origin)?;
            Ok(Outcome {
                result: encode(&outcome.result)?,
                event: outcome.event,
            })
        };
        Self {
            name,
            version,
            summary,
            signature,
            handler: Handler::Command(Box::new(erased)),
        }
    }

    /// A query named `name`, described in one sentence by `summary`, whose parameters and result
    /// `signature` describes, that runs `handler`.
    pub fn query<I, O>(
        name: &'static str,
        version: Version,
        summary: &'static str,
        signature: Signature,
        handler: impl Fn(&Ctx, I) -> Result<O, ApiError> + 'static,
    ) -> Self
    where
        Ctx: 'static,
        I: DeserializeOwned + 'static,
        O: Serialize + 'static,
    {
        let erased = move |ctx: &Ctx, params: Value| encode(&handler(ctx, decode(params)?)?);
        Self {
            name,
            version,
            summary,
            signature,
            handler: Handler::Query(Box::new(erased)),
        }
    }

    /// The stable name callers use, such as `model.set`.
    #[must_use]
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// The version of the input and output.
    #[must_use]
    pub fn version(&self) -> Version {
        self.version
    }

    /// One sentence on what it does.
    #[must_use]
    pub fn summary(&self) -> &'static str {
        self.summary
    }

    /// The JSON Schema of its parameters and result.
    #[must_use]
    pub fn signature(&self) -> &Signature {
        &self.signature
    }

    /// Whether it is a command or a query.
    #[must_use]
    pub fn kind(&self) -> Kind {
        match self.handler {
            Handler::Command(_) => Kind::Command,
            Handler::Query(_) => Kind::Query,
        }
    }

    /// Runs it on `ctx` with JSON `params`. `origin` is recorded for a command and ignored by a
    /// query.
    ///
    /// # Errors
    ///
    /// [`ApiError::InvalidParams`] when `params` do not decode into the input, or whatever the
    /// handler returns.
    pub fn call(&self, ctx: &mut Ctx, params: Value, origin: &Origin) -> Result<Outcome, ApiError> {
        match &self.handler {
            Handler::Command(run) => run(ctx, params, origin),
            Handler::Query(run) => Ok(Outcome {
                result: run(ctx, params)?,
                event: None,
            }),
        }
    }
}

/// The parameters of a call that takes none: an empty object or array.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NoParams {}

pub(crate) fn decode<I: DeserializeOwned>(params: Value) -> Result<I, ApiError> {
    serde_json::from_value(params).map_err(|error| ApiError::InvalidParams {
        detail: error.to_string(),
    })
}

fn encode<O: Serialize>(result: &O) -> Result<Value, ApiError> {
    serde_json::to_value(result).map_err(|_| ApiError::Internal)
}

#[cfg(test)]
mod tests;
