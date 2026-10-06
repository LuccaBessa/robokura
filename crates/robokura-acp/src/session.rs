use std::{
    path::PathBuf,
    str::FromStr,
    sync::{Arc, Mutex},
};

use agent_client_protocol::{
    Agent, Client, ConnectionTo, Error,
    schema::{
        ProtocolVersion,
        v1::{
            CancelNotification, ContentBlock, InitializeRequest, NewSessionRequest,
            PermissionOptionKind, PromptRequest, RequestPermissionOutcome,
            RequestPermissionRequest, RequestPermissionResponse, SelectedPermissionOutcome,
            SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory,
            SessionConfigSelectOptions, SessionNotification, SessionUpdate,
            SetSessionConfigOptionRequest, StopReason, TextContent, ToolCallStatus,
        },
    },
};
use tokio::sync::{
    mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    oneshot,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Start {
    pub command: String,
    /// The agent's own folder, so it starts inside its own directory rather than
    /// somewhere it was not invited.
    pub cwd: PathBuf,
    /// What each selector is set to, by the agent's own word for the selector.
    ///
    /// Applied only for selectors the agent actually offers. A choice stored against
    /// an agent that has since dropped it is dropped rather than sent: the protocol
    /// has no way to say "set this to a thing you no longer have".
    pub config: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector {
    pub id: String,
    pub name: String,
    /// An empty string is an agent that did not say, which is not the same as one that
    /// said something we have no word for.
    pub category: String,
    pub values: Vec<(String, String)>,
    pub current: String,
}

/// The option ids come from the agent and are not the same string in every one, so
/// they are carried through rather than assumed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub option_id: String,
    pub name: String,
    /// True when this answer allows the action rather than refusing it.
    pub allows: bool,
}

#[derive(Clone, Debug)]
pub enum Event {
    /// The agent started, and named itself.
    Ready {
        name: String,
        version: String,
    },
    /// The agent asked for something only the person can allow.
    NeedsPermission {
        id: u64,
        title: String,
        choices: Vec<Choice>,
    },
    /// The agent's own reasoning. The person can see it and need not read it.
    Thought(String),
    /// Part of the reply, arriving as it is produced.
    Text(String),
    ToolStarted {
        id: String,
        title: String,
    },
    ToolStatus {
        id: String,
        title: String,
        status: String,
    },
    Finished {
        reason: String,
    },
    Failed(String),
    /// Sent once, after the session exists and before anything is asked of it, so a
    /// person can be offered the choices an agent actually has rather than a list this
    /// application wrote down.
    Selectors(Vec<Selector>),
    /// The agent process ended, expected or not.
    Disconnected,
}

/// A live conversation with one agent.
pub struct Session {
    commands: UnboundedSender<Command>,
    events: UnboundedReceiver<Event>,
    answers: Arc<Mutex<Answers>>,
    runtime: Option<tokio::runtime::Runtime>,
}

#[derive(Default)]
struct Answers {
    waiting: Vec<(u64, oneshot::Sender<Option<String>>)>,
}

impl Answers {
    fn ask(&mut self, id: u64) -> oneshot::Receiver<Option<String>> {
        let (tx, rx) = oneshot::channel();
        self.waiting.push((id, tx));
        rx
    }

    fn answer(&mut self, id: u64, option_id: Option<String>) {
        let Some(position) = self.waiting.iter().position(|(pending, _)| *pending == id) else {
            return;
        };
        let (_, sender) = self.waiting.swap_remove(position);
        let _ = sender.send(option_id);
    }
}

enum Command {
    Prompt(String),
    Cancel,
}

/// Generous enough that an agent which was asked to stop has normally gone by the time
/// this is over, and short enough that one which will not stop does not hold up a
/// window.
const SHUTDOWN: std::time::Duration = std::time::Duration::from_millis(250);

impl Session {
    pub fn start(start: Start) -> Result<Self, String> {
        if !crate::command::exists(&start.command) {
            return Err(format!(
                "The agent could not be found on this machine: {}",
                start.command
            ));
        }

        let agent = agent_client_protocol::AcpAgent::from_str(&start.command)
            .map_err(|error| format!("could not read the agent command: {error}"))?;

        let (event_tx, events) = unbounded_channel::<Event>();
        let (command_tx, command_rx) = unbounded_channel::<Command>();
        let answers = Arc::new(Mutex::new(Answers::default()));

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("could not start the agent runtime: {error}"))?;

        runtime.spawn(run_client(
            agent,
            start,
            event_tx,
            command_rx,
            answers.clone(),
        ));

        Ok(Self {
            commands: command_tx,
            events,
            answers,
            runtime: Some(runtime),
        })
    }

    pub fn next_event(&mut self) -> Option<Event> {
        self.events.try_recv().ok()
    }

    pub fn send_prompt(&self, prompt: String) {
        let _ = self.commands.send(Command::Prompt(prompt));
    }

    /// Asks the agent to stop. The turn ends by saying it was cancelled, which is not
    /// a failure.
    pub fn cancel(&self) {
        let _ = self.commands.send(Command::Cancel);
    }

    /// Ends the agent and returns once the shutdown is done.
    ///
    /// Dropping a session only asks: it hands the shutdown to a thread and returns,
    /// because a runtime cannot be dropped on the thread that is drawing the window
    /// without first waiting for the connection to the agent to finish, and that wait
    /// is unbounded. Anything that goes on to touch what the agent was using has to
    /// call this instead, because until the agent has actually let go it is still
    /// holding the folder it was started in.
    ///
    /// It blocks, so it belongs on the way out of an action a person asked for and not
    /// on a timer.
    pub fn stop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_timeout(SHUTDOWN);
        }
    }

    /// The only path by which an agent is allowed to proceed. `option` is what the
    /// person chose, or `None` to refuse.
    pub fn answer_permission(&self, id: u64, option: Option<String>) {
        if let Ok(mut answers) = self.answers.lock() {
            answers.answer(id, option);
        }
    }
}

impl Drop for Session {
    /// Asks the agent to end without waiting for it to have.
    ///
    /// Dropping happens wherever the value happens to go, including on a thread that
    /// is drawing the window, and waiting there would freeze it. Anything that needs
    /// the agent to be gone before it carries on says so with [`Session::stop`]
    /// instead, and this is only the net under that.
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            std::thread::spawn(move || {
                runtime.shutdown_timeout(SHUTDOWN);
            });
        }
    }
}

async fn run_client(
    agent: agent_client_protocol::AcpAgent,
    start: Start,
    event_tx: UnboundedSender<Event>,
    mut commands: UnboundedReceiver<Command>,
    answers: Arc<Mutex<Answers>>,
) {
    let outcome = Client
        .builder()
        .name("robokura")
        .on_receive_notification(
            {
                let event_tx = event_tx.clone();
                async move |notification: SessionNotification, _connection| {
                    forward_update(&notification.update, &event_tx);
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            {
                let event_tx = event_tx.clone();
                let answers = answers.clone();
                async move |request: RequestPermissionRequest, responder, _connection| {
                    let id = next_permission_id();
                    let title = request
                        .tool_call
                        .fields
                        .title
                        .clone()
                        .unwrap_or_else(|| "Do something".to_string());

                    let choices = request
                        .options
                        .iter()
                        .map(|option| Choice {
                            option_id: option.option_id.0.to_string(),
                            name: option.name.clone(),
                            allows: matches!(
                                option.kind,
                                PermissionOptionKind::AllowOnce | PermissionOptionKind::AllowAlways
                            ),
                        })
                        .collect();

                    let waiting = answers
                        .lock()
                        .map_err(|_| Error::internal_error().data("permission state unavailable"))?
                        .ask(id);

                    let _ = event_tx.send(Event::NeedsPermission { id, title, choices });

                    let chosen = waiting.await.unwrap_or(None);
                    let outcome = match chosen {
                        Some(option_id) => RequestPermissionOutcome::Selected(
                            SelectedPermissionOutcome::new(option_id),
                        ),
                        None => RequestPermissionOutcome::Cancelled,
                    };
                    responder.respond(RequestPermissionResponse::new(outcome))?;
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(agent, {
            let event_tx = event_tx.clone();
            move |connection: ConnectionTo<Agent>| async move {
                serve(connection, start, event_tx, &mut commands).await
            }
        })
        .await;

    if let Err(error) = outcome {
        let _ = event_tx.send(Event::Failed(error.to_string()));
    }
    let _ = event_tx.send(Event::Disconnected);
}

async fn serve(
    connection: ConnectionTo<Agent>,
    start: Start,
    event_tx: UnboundedSender<Event>,
    commands: &mut UnboundedReceiver<Command>,
) -> Result<(), Error> {
    let initialize = connection
        .send_request(InitializeRequest::new(ProtocolVersion::V1))
        .block_task()
        .await?;

    if let Some(info) = &initialize.agent_info {
        let _ = event_tx.send(Event::Ready {
            name: info.name.to_string(),
            version: info.version.clone(),
        });
    }

    let session = connection
        .send_request(NewSessionRequest::new(start.cwd))
        .block_task()
        .await?;
    let session_id = session.session_id.clone();

    // What the agent says it can be set to, then what the person chose for it. The
    // two are in that order because a choice is only meaningful against a list.
    let offered = read_selectors(session.config_options.as_deref().unwrap_or_default());
    if !offered.is_empty() {
        let _ = event_tx.send(Event::Selectors(offered.clone()));
    }
    for (config_id, value_id) in start.config {
        if !offered.iter().any(|selector| selector.id == config_id) {
            continue;
        }
        let _ = connection
            .send_request(SetSessionConfigOptionRequest::new(
                session_id.clone(),
                config_id,
                value_id.as_str(),
            ))
            .block_task()
            .await;
    }

    while let Some(command) = commands.recv().await {
        match command {
            Command::Prompt(text) => {
                let result = connection
                    .send_request(PromptRequest::new(
                        session_id.clone(),
                        vec![ContentBlock::Text(TextContent::new(text))],
                    ))
                    .block_task()
                    .await;

                match result {
                    Ok(response) => {
                        let _ = event_tx.send(Event::Finished {
                            reason: describe_stop(response.stop_reason),
                        });
                    }
                    Err(error) => {
                        let _ = event_tx.send(Event::Failed(error.to_string()));
                    }
                }
            }
            Command::Cancel => {
                let _ = connection.send_notification(CancelNotification::new(session_id.clone()));
            }
        }
    }

    Ok(())
}

/// Both shapes an agent can answer in are read, because a flat list and a list under
/// headings are both valid. A selector that is a switch rather than a list is left
/// out: this application has no way to ask a person about one mid-conversation.
fn read_selectors(options: &[SessionConfigOption]) -> Vec<Selector> {
    options
        .iter()
        .filter_map(|option| {
            let SessionConfigKind::Select(select) = &option.kind else {
                return None;
            };

            let mut values: Vec<(String, String)> = Vec::new();
            match &select.options {
                SessionConfigSelectOptions::Ungrouped(options) => {
                    values.extend(
                        options
                            .iter()
                            .map(|value| (value.value.0.to_string(), value.name.clone())),
                    );
                }
                SessionConfigSelectOptions::Grouped(headed) => {
                    // The headings are dropped rather than worked around: the control
                    // this ends up in is a flat menu, and a heading inside one would be
                    // a line a person could read but not press.
                    for group in headed {
                        values.extend(
                            group
                                .options
                                .iter()
                                .map(|value| (value.value.0.to_string(), value.name.clone())),
                        );
                    }
                }
                // The shape list is open. A shape this version has never heard of leaves
                // the selector with nothing in it, and it is left out below rather than
                // shown as an empty choice.
                _ => {}
            }

            // A chooser that opens onto an empty list reads as something having gone
            // wrong.
            if values.is_empty() {
                return None;
            }

            Some(Selector {
                id: option.id.0.to_string(),
                name: option.name.clone(),
                category: match &option.category {
                    Some(SessionConfigOptionCategory::Mode) => "mode".to_string(),
                    Some(SessionConfigOptionCategory::Model) => "model".to_string(),
                    Some(SessionConfigOptionCategory::ModelConfig) => "model_config".to_string(),
                    Some(SessionConfigOptionCategory::ThoughtLevel) => "thought_level".to_string(),
                    Some(SessionConfigOptionCategory::Other(name)) => name.clone(),
                    // The category list is open: an agent may add one. An unknown
                    // category is kept as nothing rather than guessed at, because this
                    // application has no use for a category it cannot name.
                    Some(_) | None => String::new(),
                },
                current: select.current_value.0.to_string(),
                values,
            })
        })
        .collect()
}

fn forward_update(update: &SessionUpdate, event_tx: &UnboundedSender<Event>) {
    match update {
        SessionUpdate::AgentMessageChunk(chunk) => {
            if let ContentBlock::Text(text) = &chunk.content {
                let _ = event_tx.send(Event::Text(text.text.clone()));
            }
        }
        SessionUpdate::AgentThoughtChunk(chunk) => {
            if let ContentBlock::Text(text) = &chunk.content {
                let _ = event_tx.send(Event::Thought(text.text.clone()));
            }
        }
        SessionUpdate::ToolCall(call) => {
            let _ = event_tx.send(Event::ToolStarted {
                id: call.tool_call_id.to_string(),
                title: call.title.clone(),
            });
        }
        SessionUpdate::ToolCallUpdate(update) => {
            if let Some(status) = update.fields.status {
                let _ = event_tx.send(Event::ToolStatus {
                    id: update.tool_call_id.to_string(),
                    title: update.fields.title.clone().unwrap_or_default(),
                    status: describe_tool_status(status),
                });
            }
        }
        _ => {}
    }
}

fn describe_tool_status(status: ToolCallStatus) -> String {
    match status {
        ToolCallStatus::Pending => "waiting".to_string(),
        ToolCallStatus::InProgress => "running".to_string(),
        ToolCallStatus::Completed => "done".to_string(),
        ToolCallStatus::Failed => "failed".to_string(),
        other => format!("{other:?}").to_lowercase(),
    }
}

fn describe_stop(reason: StopReason) -> String {
    match reason {
        StopReason::EndTurn => "done".to_string(),
        StopReason::MaxTokens => "out of tokens".to_string(),
        StopReason::MaxTurnRequests => "too many steps".to_string(),
        StopReason::Refusal => "declined".to_string(),
        StopReason::Cancelled => "cancelled".to_string(),
        other => format!("{other:?}").to_lowercase(),
    }
}

fn next_permission_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}
