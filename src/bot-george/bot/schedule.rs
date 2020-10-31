use docbot::Docbot;

#[derive(Docbot, Debug)]
/// TODO
pub enum ScheduleCommand {
    /// help [command]
    /// Get help with scheduling, or a particular schedule subcommand
    /// 
    /// # Arguments
    /// command: The name of a subcommand to get info for
    Help(Option<ScheduleCommandId>),
}
