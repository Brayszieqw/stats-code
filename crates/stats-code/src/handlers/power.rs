use crate::cli::PowerCommand;
use crate::power::{power_one_proportion, power_two_means, power_two_proportions};
use crate::schema::PowerResult;

pub(crate) fn handle_power(command: &PowerCommand) -> Result<PowerResult, String> {
    match command {
        PowerCommand::OneProportion(args) => power_one_proportion(args),
        PowerCommand::TwoProportions(args) => power_two_proportions(args),
        PowerCommand::TwoMeans(args) => power_two_means(args),
    }
}
