//! Hand-written Mermaid fixtures shared by the tests. Every text here was written for this
//! repository; none is copied from another project.
#![allow(dead_code)]

use vizcraft_mermaid::{Parsed, parse};

/// Parse and fail the test with the error text when parsing fails.
pub fn ok(text: &str) -> Parsed {
    match parse(text) {
        Ok(p) => p,
        Err(e) => panic!("parse failed: {e}\n---\n{text}"),
    }
}

pub const FLOWCHART: &str = r#"---
title: Order handling
---
%%{init: {"theme": "neutral"}}%%
flowchart LR
    %% entry point
    start([Order received]) --> check{In stock?}
    check -- yes --> pack[[Pack items]]
    check -->|no| back[(Backorder table)]
    pack ==> ship>Ship it] -.-> done(((Done)))
    back --o notify((Notify)) & log[/Write log/]
    log --x audit[\Audit\]
    a1[/Wide base\] <--> a2[\Narrow base/]
    hex{{Prepare}} ~~~ par[/Lean right/]
    subgraph warehouse [Warehouse floor]
        direction TB
        pack
        subgraph dock["Loading dock"]
            ship
        end
    end
    classDef urgent fill:#f96,stroke:#333,stroke-width:2px
    class check,back urgent
    done:::urgent
    style notify fill:#bbf,stroke-dasharray: 5 5
    linkStyle 0 stroke:#ff3,stroke-width:4px
    linkStyle default interpolate basis stroke:#999
    click start "https://example.com/orders" "Open orders" _blank
"#;

pub const STATE: &str = r#"stateDiagram-v2
    direction LR
    [*] --> Idle
    Idle --> Running : start
    state "Running the job" as Running
    Running : progress shown
    Running --> Paused : pause
    Paused --> Running : resume
    state Check <<choice>>
    Running --> Check
    Check --> Done : ok
    Check --> Failed : error
    state Split <<fork>>
    state Merge <<join>>
    Done --> Split
    Split --> A
    Split --> B
    A --> Merge
    B --> Merge
    Merge --> [*]
    state Active {
        [*] --> Working
        Working --> Waiting
        --
        [*] --> Logging
        Logging --> [*]
    }
    note right of Idle : waiting for input
    note left of Paused
        paused by the user
        or by a timeout
    end note
    classDef bad fill:#f00
    class Failed bad
    Done:::good
"#;

pub const ER: &str = r#"erDiagram
    direction LR
    CUSTOMER ||--o{ ORDER : places
    ORDER ||--|{ LINE-ITEM : contains
    CUSTOMER }|..|{ DELIVERY-ADDRESS : uses
    p[Person] {
        string name PK "full name"
        int age
        string email UK
        int address_id FK, UK "points at ADDRESS"
    }
    CUSTOMER {
        string id PK
        varchar(255) name
    }
    PRODUCT |o--o| ORDER : "may be in"
    PRODUCT one or more to zero or many CATEGORY : "filed under"
    PERSON only one optionally to 1+ CAR : drives
"#;

pub const CLASS: &str = r#"classDiagram
    direction RL
    class Animal {
        <<abstract>>
        +String name
        -int age$
        +makeSound()* void
        +getAge() int
    }
    class Duck["Mallard duck"] {
        +swim()
        +quack(List~int~ volumes) List~string~
    }
    class Square~Shape~
    Animal <|-- Duck
    Animal "1" *-- "many" Leg : has
    Pond o-- Duck
    Duck --> Food : eats
    Duck ..> Water
    Flyer ..|> Duck
    Duck -- Wing
    Duck .. Feather
    Shape ()-- Square
    <<interface>> Flyer
    Animal : +breathe() bool
    namespace Zoo {
        class Keeper
        class Cage {
            +int size
        }
    }
    note "A small farm"
    note for Duck "can fly\ncan swim"
    classDef hot fill:#f00
    cssClass "Duck,Animal" hot
    style Pond fill:#0af
    link Duck "https://example.com/duck" "About ducks"
"#;

pub const SEQUENCE: &str = r#"sequenceDiagram
    title: Checkout
    autonumber
    box Aqua Front end
        actor User
        participant W as Web app
    end
    participant API
    User->>W: open cart
    W->>+API: GET /cart
    API-->>-W: 200 OK
    W-)API: track event
    W-xAPI: cancel
    W--xAPI: cancel quietly
    W--)API: async dotted
    W->API: plain
    W-->API: plain dotted
    User<<->>W: talk
    loop every minute
        W->>API: ping
    end
    alt paid
        API->>W: receipt
    else declined
        API->>W: error
    end
    opt promo
        W->>API: apply code
    end
    par first
        W->>API: a
    and second
        W->>API: b
    end
    critical connect
        API->>DB: open
    option timeout
        API->>W: retry later
    end
    break failure
        API->>W: abort
    end
    rect rgb(200, 220, 255)
        W->>API: inside rect
    end
    Note right of W: thinking
    Note over W,API: handshake
    activate API
    deactivate API
    create participant Log
    API->>Log: write
    destroy Log
"#;

/// Every fixture, for the hostile-input test.
pub const ALL: &[&str] = &[FLOWCHART, STATE, ER, CLASS, SEQUENCE];
