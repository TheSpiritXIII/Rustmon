use std::cmp::Ordering;

use rand::{Rng, StdRng};

use base::monster::Monster;
use base::attack::target;
use base::party::{Party, PartyMember};
pub use base::command::{Command, CommandType, CommandAttack, CommandSwitch};
pub use base::effect::{Effect, NoneReason};
pub use base::types::battle::StatModifierType;
pub use base::statmod::StatModifiers;

// TODO: This class is redundant. Break it up
#[derive(Debug)]
struct BattleCommand
{
	command: Command,
	effects: Vec<Effect>,
}

impl BattleCommand
{
	fn with_miss(command: Command) -> Self
	{
		let mut effects = Vec::new();
		effects.push(Effect::None(NoneReason::Miss));
		BattleCommand
		{
			effects: effects,
			command: command,
		}
	}
	fn new<'a, R: Rng>(command: Command, parties: &[Party<'a>], rng: &mut R) -> Self
	{
		let mut effects = Vec::new();
		command.command_type.effects(parties, &command, rng, &mut effects);
		BattleCommand
		{
			effects: effects,
			command: command,
		}
	}
}

pub struct Battle<'a>
{
	/// Maps to the groups that have already added a command.
	ready: Vec<Vec<Option<usize>>>,

	/// True if the turn has started, false otherwise.
	started: bool,

	/// The total number of available participants.
	total: usize,

	/// The number of groups still waiting for a command.
	waiting: usize,

	/// The parties in this battle.
	parties: Vec<Party<'a>>,

	/// The list of executed commands, for playback.
	commands: Vec<BattleCommand>,

	/// The queue of upcoming commands in the next turn.
	queue: Vec<Command>,

	/// The current effect being executed.
	current: usize,

	rng: StdRng,

	switch_queue: Option<(usize, usize)>,

	switch_waiting: usize,

	// TODO: lingering effects.
}

pub enum BattleExecution
{
	Command,
	Queue,
	Waiting,
	Switch(usize),
	SwitchWaiting,
}

/// Indicates an error adding a command to a battle.
#[derive(Debug, PartialEq)]
pub enum BattleError
{
	/// There was no error.
	None,
	/// Occurs when the battle turn is in progress. New commands cannot be added.
	Blocking,
	/// Occurs when the chosen attack is unable to be used due to the limit.
	Limit,
	/// Occurs when the chosen attack is unable to target the chosen party and respective member.
	Target,
	/// Occurs when a switch cannot occur.because the target is already active.
	Active,
	/// Occurs when a switch cannot occur because the target has no health.
	Health,
	/// Occurs when a switch cannot occur because the target has already been queued to switch.
	Queued,
	/// Occurs when an escape cannot occur because the party has already added commands.
	Escape,
}

fn is_adjacent_with(to: usize, from: usize) -> bool
{
	to == from || (to > 0 && to - 1 == from) || (to < usize::max_value() && to + 1 == from)
}

impl<'a> Battle<'a>
{
	pub fn new(parties: Vec<Party<'a>>) -> Self
	{
		let mut total = 0;
		let mut ready = Vec::with_capacity(parties.len());
		for group in &parties
		{
			total += group.active_count();
			ready.push(vec![None; group.active_count()]);
		}
		Battle
		{
			ready: ready,
			started: false,
			total: total,
			waiting: total,
			parties: parties,
			commands: Vec::new(),
			queue: Vec::new(),
			current: 0,
			rng: StdRng::new().unwrap(),
			switch_queue: None,
			switch_waiting: 0,
		}
	}
	pub fn party(&self, index: usize) -> &Party<'a>
	{
		&self.parties[index]
	}
	pub fn monster(&self, party: usize, monster: usize) -> &Monster
	{
		self.parties[party].member(monster)
	}
	pub fn monster_active(&self, party: usize, monster: usize) -> Option<PartyMember>
	{
		self.parties[party].active_member(monster)
	}
	pub fn monster_active_alive(&self, party: usize, monster: usize) -> Option<PartyMember>
	{
		self.parties[party].active_member_alive(monster)
	}
	pub fn monster_is_active(&self, party: usize, monster: usize) -> bool
	{
		self.parties[party].member_is_active(monster)
	}
	pub fn monster_active_count(&self, party: usize) -> usize
	{
		self.parties[party].active_count()
	}
	fn is_member_valid(&self, party: usize, member: usize) -> BattleError
	{
		assert!(party < self.parties.len());
		assert!(member < self.parties[party].member_count());
		BattleError::None
	}
	fn is_switch_valid(&self, party: usize, member: usize) -> BattleError
	{
		let err = self.is_member_valid(party, member);
		if err != BattleError::None
		{
			err
		}
		else if self.monster_is_active(party, member)
		{
			BattleError::Active
		}
		else if self.monster(party, member).get_health() == 0
		{
			BattleError::Health
		}
		else
		{
			BattleError::None
		}
	}
	fn is_command_valid(&self, party: usize, member: usize) -> BattleError
	{
		if self.started
		{
			BattleError::Blocking
		}
		else
		{
			self.is_member_valid(party, member)
		}
	}
	pub fn add_command_attack(&mut self, party: usize, member: usize, target_party: usize,
		target_member: usize, attack_index: usize) -> BattleError
	{
		println!("Added attack {}", attack_index);

		let err = self.is_command_valid(party, member);
		if err != BattleError::None
		{
			return err;
		}

		let attack_command = CommandAttack
		{
			member: member,
			target_party: target_party,
			target_member: target_member,
			attack_index: attack_index,
		};

		{
			let monster_attack = attack_command.attack(party, self);
			if monster_attack.limit_left() == 0
			{
				return BattleError::Limit;
			}

			let attack = monster_attack.attack();

			let same_party = party == attack_command.target_party;
			if (attack.target & target::SIDE_ENEMY) == 0 && !same_party
			{
				return BattleError::Target;
			}
			if (attack.target & target::SIDE_ALLY) == 0 && same_party
			{
				return BattleError::Target;
			}

			let is_adjacent = is_adjacent_with(attack_command.member, attack_command.target_member);
			if (attack.target & target::RANGE_ADJACENT) == 0 && is_adjacent
			{
				return BattleError::Target;
			}
			if (attack.target & target::RANGE_OPPOSITE) == 0 && !is_adjacent
			{
				return BattleError::Target;
			}

			let same_member = attack_command.member == attack_command.target_member;
			if (attack.target & target::TARGET_SELF) == 0 && same_party && same_member
			{
				return BattleError::Target;
			}
		}

		self.add_command_to_queue(party, member, CommandType::Attack(attack_command));
		BattleError::None
	}

	pub fn add_command_switch(&mut self, party: usize, member: usize, target: usize) -> BattleError
	{
		let err = self.is_command_valid(party, member);
		if err != BattleError::None
		{
			return err;
		}

		let switch_err = self.is_switch_valid(party, target);
		if switch_err != BattleError::None
		{
			switch_err
		}
		else
		{
			// TODO: Optimize queue switch check?
			let queued = self.queue.iter().any(|command|
			{
				if command.party() == party
				{
					if let CommandType::Switch(ref switch_command_other) = command.command_type
					{
						if switch_command_other.target == target
						{
							return true;
						}
					}
				}
				false
			});
			if !queued
			{
				let switch_command = CommandSwitch
				{
					member: member,
					target: target,
				};
				self.add_command_to_queue(party, member, CommandType::Switch(switch_command));
				BattleError::None
			}
			else
			{
				BattleError::Queued
			}
		}
	}
	pub fn add_command_escape(&mut self, party: usize) -> BattleError
	{
		assert!(party < self.parties.len());

		let member_queued = self.ready[party].iter().any(|member|
		{
			member.is_some()
		});
		if member_queued
		{
			BattleError::Escape
		}
		else
		{
			for member in &mut self.ready[party]
			{
				// Delete any existing commands if they exist.
				if let Some(queue_index) = *member
				{
					// TODO: NOTE: This invalidates all other indices! Fix ASAP!
					self.queue.remove(queue_index);
				}
				*member = Some(self.queue.len());
			}
			self.waiting -= self.ready[party].len();

			self.queue.push(Command::new(CommandType::Escape, party));
			BattleError::None
		}
	}
	fn add_command_to_queue(&mut self, party: usize, member: usize, command: CommandType)
	{
		if let Some(queue_index) = self.ready[party][member]
		{
			debug_assert!(self.queue[queue_index].party() == party);
			self.queue[queue_index].command_type = command;
		}
		else
		{
			self.ready[party][member] = Some(self.queue.len());
			self.waiting -= 1;

			self.queue.push(Command::new(command, party));
		}
	}

	pub fn execute_post_switch(&mut self, party: usize, member: usize, target: usize) -> BattleError
	{
		let err = self.is_switch_valid(party, target);
		if err != BattleError::None
		{
			err
		}
		else
		{
			self.switch(party, member, target);
			// self.parties[party].active_set(member, target);
			self.switch_waiting -= 1;
			BattleError::None
		}
	}

	pub fn is_party_post_switch_waiting(&self, party: usize) -> Option<usize>
	{
		self.parties[party].switch_waiting()
	}

	pub fn execute_switch(&mut self, member: usize) -> BattleError
	{
		let (party, active) = self.switch_queue.unwrap();
		let err = self.is_switch_valid(party, member);
		if err != BattleError::None
		{
			err
		}
		else
		{
			self.switch(party, active, member);
			self.switch_queue = None;
			BattleError::None
		}
	}

	fn switch(&mut self, party: usize, member: usize, with: usize)
	{
		let p = &mut self.parties[party];
		p.switch_active(member, with);
	}

	fn execute_command(&mut self) -> BattleExecution
	{
		let mut min_index = 0;
		for index in 1..self.queue.len()
		{
			if Command::cmp(&self.queue[index], &self.queue[min_index], self) == Ordering::Less
			{
				min_index = index;
			}
		}
		let command = self.queue.swap_remove(min_index);

		let hit = if let CommandType::Attack(ref attack_command) = command.command_type
		{
			let hit =
			{
				self.parties[attack_command.target_party].active_member_alive(attack_command.target_member).is_some()
			};

			let user = self.parties[command.party()].member_mut(attack_command.member);
			user.get_attacks_mut()[attack_command.attack_index].limit_left_take(1);

			hit
		}
		else
		{
			true
		};

		self.commands.push(if hit
		{
			BattleCommand::new(command, &self.parties, &mut self.rng)
		}
		else
		{
			BattleCommand::with_miss(command)
		});
		self.current = 0;
		BattleExecution::Command
	}

	/// Executes the next battle action.
	pub fn execute(&mut self) -> BattleExecution
	{
		if self.started
		{
			if let Some(switch_party) = self.switch_queue
			{
				BattleExecution::Switch(switch_party.0)
			}
			else if self.current != self.commands.last().unwrap().effects.len()
			{
				self.apply_effect();

				self.current += 1;
				BattleExecution::Queue
			}
			else if !self.queue.is_empty()
			{
				self.execute_command()
			}
			else if self.switch_waiting != 0
			{
				BattleExecution::SwitchWaiting
			}
			else
			{
				for x in 0..self.parties.len()
				{
					let party = self.parties.get_mut(x).unwrap();
					let mut i = 0;
					while i != party.active_count()
					{
						if party.active_member(i).is_none()
						{
							party.active_remove(i);
						}
						else
						{
							i += 1;
						}
					}
				}

				// Reset the waiting for new commands.
				self.waiting = self.total;
				self.started = false;
				for ready_party in &mut self.ready
				{
					for ready in ready_party.iter_mut()
					{
						*ready = None;
					}
				}
				BattleExecution::Waiting
			}
		}
		else if self.waiting != 0
		{
			BattleExecution::Waiting
		}
		else
		{
			// TODO: Insert lingering effects into priority queue.

			self.started = true;
			self.execute_command()
		}
	}

	/// The current executing command.
	pub fn get_current_command(&self) -> Option<&Command>
	{
		self.commands.last().map(|battle_command| &battle_command.command)
	}

	/// The current executing result of the current executing command.
	pub fn get_current_effect(&self) -> Option<&Effect>
	{
		self.commands.last().map(|battle_command| &battle_command.effects[self.current - 1])
	}

	fn affects_member(command: &Command, member: usize) -> bool
	{
		match command.command_type
		{
			CommandType::Attack(ref attack_command) =>
			{
				attack_command.member == member
			}
			CommandType::Switch(ref switch_command) =>
			{
				switch_command.member == member
			}
			CommandType::Escape =>
			{
				false
			}
		}
	}

	fn apply_effect(&mut self)
	{
		let battle_command = self.commands.last().unwrap();
		match battle_command.effects[self.current]
		{
			Effect::Damage(ref effect) =>
			{
				let member = effect.active;

				let dead =
				{
					let target = self.parties[effect.party()].member_mut(member);
					target.lose_health(effect.amount());
					println!("Lost health: {}, {}, {}", target.get_health(), effect.active, effect.party());
					target.get_health() == 0
				};

				if dead
				{
					for i in 0..self.queue.len()
					{
						if self.queue[i].party() == effect.party() &&
							Battle::affects_member(&self.queue[i], effect.active)
						{
							println!("Removing queue: {}", i);
							println!("Queue: {:?}", self.queue);
							self.queue.swap_remove(i);
							break;
						}
					}

					let party = self.parties.get_mut(effect.party()).unwrap();

					for i in 0..party.member_count()
					{
						// TODO: Maybe cache amount of party members left?
						if party.member(i).get_health() != 0 && !party.member_is_active(i)
						{
							self.switch_waiting += 1;
							return;
						}
					}

					party.active_reset(effect.active);

					// At this point, it doesn't matter which we remove because they're all false.
					self.ready[effect.party()].pop();
					self.total -= 1;
				}
			}
			Effect::Switch(ref switch) =>
			{
				let p = &mut self.parties[battle_command.command.party()];
				p.switch_active(switch.member, switch.target);
				// self.switch(battle_command.command.party, battle_command.command.monster, target);
			}
			Effect::Modifier(ref modifiers) =>
			{
				let party = self.parties.get_mut(modifiers.party()).unwrap();
				party.active_member_modifiers_add(modifiers.active(), modifiers.modifiers());
			}
			Effect::None(_) => ()
		}
	}
}
