extern crate mon_gen;
extern crate rand;

mod display;
mod terminal;

use std::str;

use mon_gen::{SpeciesType, Monster};
use mon_gen::base::battle::{Battle, Party, CommandType, BattleExecution, CommandAttack, Effect, Damage};

use display::{display_attacks, display_party, display_active};

use rand::distributions::{Range, IndependentSample};

fn main()
{
	let mut rng = rand::thread_rng();

	let mut party_enemy = vec![
		Monster::new(SpeciesType::Deoxys, 10),
		Monster::new(SpeciesType::Deoxys, 10),
	];
	let mut party_self = vec![
		Monster::new(SpeciesType::Shaymin, 10),
		Monster::new(SpeciesType::Bulbasaur, 5),
	];
	let battle_self = Party::new(&mut party_self, 2);
	let battle_enemy = Party::new(&mut party_enemy, 2);
	let battle_data = vec![
		battle_self,
		battle_enemy,
	];

	let mut battle = Battle::new(battle_data);

	let mut last_input: Option<usize> = None;
	let mut active = 0;
	loop
	{
		terminal::clear();
		println!("");
		display_active(&battle, active);
		println!("Inputting...");

		if let Some(input) = last_input
		{
			match input
			{
				1 =>
				{
					let attack_len =
					{
						let attack_list = battle.monster_active(0, active).get_attacks();
						display_attacks(attack_list);
						attack_list.len() + 1
					};
					println!("\nChoose an attack to use:");
					let input = terminal::input_range(attack_len);
					if input == attack_len
					{
						last_input = None;
						continue;
					}
					else
					{
						let attack_command = CommandAttack
						{
							party: 1,
							monster: 0,
							attack_index: input - 1,
						};
						battle.add_command(CommandType::Attack(attack_command), 0, active);
					}
				}
				3 =>
				{
					display_party(battle.party(0).members);
					println!("\nChoose a party member to switch to:");
					let input = terminal::input_range(battle.party(0).members.len() + 1);
					if input == battle.party(0).members.len() + 1
					{
						last_input = None;
						continue;
					}
					else
					{
						println!("Unimplemented Switch");
					}
				}
				4 =>
				{
					// TODO: Escape calculation.
					println!("Ran away safely.");
					break;
				}
				_ => println!("Unimplemented action"),
			}

			if active != battle.monster_active_count(0) - 1
			{
				// Select command for next monster.
				active += 1;
				last_input = None;
				continue;
			}
			else
			{
				let target_range = Range::new(0, battle.monster_active_count(0));

				// AI battle command.
				for opponent_index in 0..battle.monster_active_count(1)
				{
					let attack_range = Range::new(0,
						battle.monster_active(1, active).get_attacks().len());
					let attack_command = CommandAttack
					{
						party: 0,
						monster: target_range.ind_sample(&mut rng),
						attack_index: attack_range.ind_sample(&mut rng),
					};
					battle.add_command(CommandType::Attack(attack_command), 1, opponent_index);
				}

				active = 0;
			}

			loop
			{
				terminal::clear();
				println!("");
				display_active(&battle, usize::max_value());
				println!("");

				match battle.execute()
				{
					BattleExecution::Command =>
					{
						let command = battle.get_current_command().unwrap();
						match command.command_type
						{
							CommandType::Attack(_) =>
							{
								let monster = &battle.party(
									command.party).members[command.monster];
								let nick = str::from_utf8(monster.get_nick()).unwrap();
								println!("{} used an attack.", nick);
							}
							_ =>
							{
								// println!("Unknown command : {:?}", command);
							}
						}
						terminal::wait();
					}
					BattleExecution::Queue =>
					{
						let effect = battle.get_current_effect().unwrap();
						match *effect
						{
							Effect::Damage(ref damage) =>
							{
								let member = battle.monster(damage.party(), damage.member());
								if member.get_health() == 0
								{
									terminal::clear();
									println!("");
									display_active(&battle, usize::max_value());
									println!("");
									println!("{} fainted!",
										str::from_utf8(member.get_nick()).unwrap());
									terminal::wait();
								}
							}
							_ =>
							{
								// Ignore.
							}
						}
						continue;
					}
					BattleExecution::Waiting =>
					{
						break;
					}
				}
			}
			last_input = None;
			continue;
		}
		else
		{
			println!("{:^20}{:^20}{:^20}{:^20}", "1) Attack", "2) Item", "3) Switch", "4) Exit");
			println!("\nWhat will you do?");
		}


		last_input = Some(terminal::input_range(4));
	}
}
