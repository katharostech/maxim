Implementation of a highly-scalable and ergonomic actor model for Rust

[![Latest version](https://img.shields.io/crates/v/maxim.svg)](https://crates.io/crates/maxim)
[![Average time to resolve an issue](https://isitmaintained.com/badge/resolution/katharostech/maxim.svg)](https://isitmaintained.com/project/katharostech/maxim)
[![License](https://img.shields.io/crates/l/maxim.svg)](https://github.com/katharostech/maxim#license)
[![Changelog.md](https://img.shields.io/badge/Keep%20a%20Changelog-Changelog.md-blue)](https://github.com/katharostech/maxim/blob/master/CHANGELOG.md)

# Maxim

Maxim is a fork of the [Axiom](https://github.com/rsimmonsjr/axiom) actor framework that was made so we could use the awesome Actor framework while experimenting with our own ideas for Actor framework design.

Maxim brings a highly-scalable actor model to the Rust language based on the many lessons learned over years of Actor model implementations in Akka and Erlang. Maxim is, however, not a direct re-implementation of either of the two aforementioned actor models but rather a new implementation deriving inspiration from the good parts of those projects.

Current development on Maxim is focused on learning how the framework works and experimenting with our design ideas. We will be pushing `0.1.0-alpha` releases with our changes until it gets to a point that is relatively usable. The first thing we've added since the fork was a `spawn_pool` feature that allows you to create pools of actors. This and other features we add are likely to change and adapt as we test them in our projects.

Other things that we are thinking about changing are:

- Using [Agnostik](https://github.com/bastion-rs/agnostik) as an executor to allow Maxim to run on any executor
  - If Agnostik will not suffice for some reason we will probably switch to Tokio for the executor to avoid maintaining our own
- Adding an optional macro for matching on message type
- Adding an option to use either bounded or unbounded channels for actor messages ( see "Design Principals of Maxim" below for more info )
  - This woud probably involve using [Flume](https://github.com/zesterer/flume) for backing the channels

## Getting Started

_An actor model is an architectural asynchronous programming paradigm characterized by the use of actors for all processing activities._

Actors have the following characteristics:

1. An actor can be interacted with only by means of messages.
2. An actor processes only one message at a time.
3. An actor will process a message only once.
4. An actor can send a message to any other actor without knowledge of that actor's internals.
5. Actors send only immutable data as messages, though they may have mutable internal state.
6. Actors are location agnostic; they can be sent a message from anywhere in the cluster.

Note that within the language of Rust, rule five cannot be enforced by Rust but is a best practice which is important for developers creating actors based on Maxim. In Erlang and Elixir rule five cannot be violated because of the structure of the language but this also leads to performance limitations. It's better to allow internal mutable state and encourage the good practice of not sending mutable messages.

What is important to understand is that these rules combined together makes each actor operate like a micro-service in the memory space of the program using them. Since actor messages are immutable, actors can trade information safely and easily without copying large data structures.

Although programming in the actor model is quite an involved process you can get started with Maxim in only a few lines of code.

```rust
use maxim::prelude::*;
use std::sync::Arc;
use std::time::Duration;

let system = ActorSystem::create(ActorSystemConfig::default().thread_pool_size(2));

let aid = system
    .spawn()
    .with(
        0 as usize,
        |state: usize, _context: Context, _message: Message| async move {
            Ok(Status::done(state))
        }
    )
    .unwrap();

aid.send(Message::new(11)).unwrap();

// It is worth noting that you probably wouldn't just unwrap in real code but deal with
// the result as a panic in Maxim will take down a dispatcher thread and potentially
// hang the system.

// This will wrap the value `17` in a Message for you!
aid.send_new(17).unwrap();

// We can also create and send separately using just `send`, not `send_new`.
let message = Message::new(19);
aid.send(message).unwrap();

// Another neat capability is to send a message after some time has elapsed.
aid.send_after(Message::new(7), Duration::from_millis(10)).unwrap();
aid.send_new_after(7, Duration::from_millis(10)).unwrap();
```

This code creates an actor system, fetches a builder for an actor via the `spawn()` method, spawns an actor and finally sends the actor a message. Once the actor is done processing a message it returns the new state of the actor and the status after handling this message. In this case we didnt change the state so we just return it. Creating an Maxim actor is literally that easy but there is a lot more functionality available as well.

Keep in mind that if you are capturing variables from the environment you will have to wrap the `async move {}` block in another block and then move your variables into the first block. Please see the test cases for more examples of this.

If you want to create an actor with a struct that is simple as well. Let's create one that handles a couple of different message types:

```rust
use maxim::prelude::*;
use std::sync::Arc;

let system = ActorSystem::create(ActorSystemConfig::default().thread_pool_size(2));

struct Data {
    value: i32,
}

impl Data {
    fn handle_bool(mut self, message: bool) -> ActorResult<Self> {
        if message {
            self.value += 1;
        } else {
            self.value -= 1;
        }
        Ok(Status::done(self))
    }

    fn handle_i32(mut self, message: i32) -> ActorResult<Self> {
        self.value += message;
        Ok(Status::done(self))
    }

    async fn handle(mut self, _context: Context, message: Message) -> ActorResult<Self> {
        if let Some(msg) = message.content_as::<bool>() {
            self.handle_bool(*msg)
        } else if let Some(msg) = message.content_as::<i32>() {
            self.handle_i32(*msg)
        } else {
            panic!("Failed to dispatch properly");
        }
    }
}

let data = Data { value: 0 };
let aid = system.spawn().name("Fred").with(data, Data::handle).unwrap();

aid.send_new(11).unwrap();
aid.send_new(true).unwrap();
aid.send_new(false).unwrap();
```

This code creates a named actor out of an arbitrary struct. Since the only requirement to make an actor is to have a function that is compliant with the [`maxim::actors::Processor`] trait, anything can be an actor. If this struct had been declared somewhere outside of your control you could use it in an actor as state by declaring your own handler function and making the calls to the 3rd party structure.

_It's important to keep in mind that the starting state is moved into the actor and you will not have external access to it afterwards._ This is by design and although you could conceivably use a [`Arc`] or [`Mutex`] enclosing a structure as state, that would definitely be a bad idea as it would break the rules we laid out for actors.

## Detailed Examples

- [Hello World](https://github.com/katharostech/maxim/blob/master/examples/hello_world.rs): The
  obligatory introduction to any computer system.
- [Dining Philosophers](https://github.com/katharostech/maxim/blob/master/examples/philosophers.rs):
  An example of using Maxim to solve a classic Finite State Machine problem in computer science.
- [Monte Carlo](https://github.com/katharostech/maxim/blob/master/examples/montecarlo.rs): An
  example of how to use Maxim for parallel computation.

## Design Principals of Maxim

These are the core principals of Axiom, the project Maxim was forked from:

1. **At its core an actor is just an function that processes messages.** The simplest actor is a
   function that takes a message and simply ignores it. The benefit to the functional approach over
   the Akka model is that it allows the user to create actors easily and simply. This is the notion
   of _micro module programming_; the notion of building a complex system from the smallest
   components. Software based on the actor model can get complicated; keeping it simple at the core
   is fundamental to solid architecture.
2. **Actors can be a Finite State Machine (FSM).** Actors receive and process messages nominally
   in the order received. However, there are certain circumstances where an actor has to change to
   another state and process other messages, skipping certain messages to be processed later.
3. **When skipping messages, the messages must not move.** Akka allows the skipping of messages
   by _stashing_ the message in another data structure and then restoring this stash later. This
   process has many inherent flaws. Instead Axiom allows an actor to skip messages in its channel
   but leave them where they are, increasing performance and avoiding many problems.
4. **Actors use a bounded capacity channel.** In Axiom the message capacity for the actor's
   channel is bounded, resulting in greater simplicity and an emphasis on good actor design.
5. **Axiom should be kept as small as possible.** Axiom is the core of the actor model and
   should not be expanded to include everything possible for actors. That should be the job of
   libraries that extend Axiom. Axiom itself should be an example of _micro module programming_.
6. **The tests are the best place for examples.** The tests of Axiom will be extensive and well
   maintained and should be a resource for those wanting to use Axiom. They should not be a dumping
   ground for copy-paste or throwaway code. The best tests will look like architected code.
7. **A huge emphasis is put on crate user ergonomics.** Axiom should be easy to use.

The principals that Maxim may **not** preserve are principals 4 and 6. To address those:

- **Bounded capacity channel:** While it may be best to have a bounded capacity channel, we will need to do some experimentation with the design before we settle our own opinion on it and our initial reaction is that it would be good to have the user be allowed to choose. As far as complexity is concerned we will probably look into out-sourcing our channel implementation to something like [Flume](https://github.com/zesterer/flume). There has not been enough investigation made to make such statements with any certainty, though.
- **The tests are the best place for examples:** While we agree that tests should be examples of how the code will actually be used, we are less along the lines of telling users to go look at the unit tests to find out how to use the library. We want the documentation to be rich and helpful to the users so that they don't _have_ to look at the tests to find out how to use the tool.

[`maxim::actors::processor`]: https://docs.rs/maxim/latest/maxim/actors/trait.Processor.html
[`arc`]: https://doc.rust-lang.org/stable/std/sync/struct.Arc.html
[`mutex`]: https://doc.rust-lang.org/stable/std/sync/struct.Mutex.html

## Dependency Graph

  - **Black:** regular dependency
  - **Red:** optional dependency

![dependency graph](./doc/dep-graph.png)

## License

Copyright 2020 Maxim Contributors

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

  http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
