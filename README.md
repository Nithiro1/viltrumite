# Viltrumite

Mod nativo (Rust) para o [Horizon](https://playhorizon.dev) (Minecraft 26.3): vire um Viltrumita, com voo até o
sônico, superforça, durabilidade e golpes. Feito do zero; mod de fã, sem ligação com a Image Comics, a Skybound ou a
Amazon, e sem código nem arquivos do mod Java "Viltrumite".

## Poderes

| Poder | Como funciona |
|---|---|
| Escolha | Ao entrar num mundo pela primeira vez, uma caixa pergunta se você quer ser Viltrumita. Depois: `/viltrumite power` ou `/viltrumite depower` (com OP: `<jogador>`) |
| Voo | Dois toques no **pulo** decolam. Pairando: WASD, pulo sobe, agachar desce. Tocar no chão pousa |
| Cruzeiro e sônico | Segure **correr** olhando pra onde quer ir: acelera até 80 m/s; a partir de 64 m/s vira **sônico** (tremor, FOV aberto, estrondo, rastro de nuvem) e abre um túnel 4×4 nos blocos à frente (somem sem drop; mirando para baixo, não quebra) |
| Dash | Slot 1 (**V**): arranque pra frente, recarga de 1 s |
| Durabilidade | Toma 20% do dano; nada de dano de queda nem de bater voando em parede |
| Força | Soco/arma corpo a corpo: 4x o dano e 4x o empurrão |
| Agarrar e arremessar | Slot 2 (**G**) pega o mob pra onde você olha; de novo arremessa; **Z** solta |
| Palmas trovejantes | Slot 3 (**R**): onda de choque num cone de 10 blocos. Recarga de 3 s |
| Impacto | Mergulhe em cruzeiro/sônico até o chão: explosão de impacto (não quebra blocos) |
| Menu de habilidades | **K**: arraste as habilidades para os 4 slots (V, G, R, B) |
| HUD e poses | Cartão de voo e velocidade, slots com recarga, e poses do personagem em 3ª pessoa, com a sua skin |
| Sons | Vento que cresce com a velocidade, estrondo sônico com trovão que ecoa (atrasado para quem está longe), túnel, golpes, impacto e pouso, com sons do próprio jogo |

Teclas em **Opções › Controles › Viltrumite**; ajustes (HUD, tremor, velocidade máxima, campo de visão, túnel,
volume dos efeitos) em **Opções › Mods › Viltrumite › Config**.

Funciona no seu mundo (single player, ou LAN que você hospeda); os poderes só existem onde o servidor tem o mod.

## Compilar

O launcher do Horizon compila o `.hmod` (este repositório compactado: `horizon-mod.toml`, `Cargo.toml`,
`Cargo.lock`, `icon.png` e `src/`) contra o build fixado em `horizon-mod.toml` (Nightly 2026-11-18, `dd67b76`).
Para desenvolver, `cargo horizon-mod` baixa o SDK em `.horizon/sdk/`, e `cargo test` roda os testes.

## Licença

MIT (veja `LICENSE`).
