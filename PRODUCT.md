# Product

<!-- impeccable:product-schema 1 -->

## Users

Pessoas que ouvem Spotify no computador e preferem controlar reprodução e playlists pelo terminal.

## Product Purpose

Riff é um player Spotify terminal-first; playlists são arquivos `.riff` legíveis, editáveis e versionáveis. Sucesso é reproduzir e gerenciar música sem esconder a playlist num banco de dados de aplicativo.

## Positioning

A playlist declarativa em texto é a fonte da verdade; a Workbench é a interface visual para trabalhar sobre esse mesmo arquivo.

## Operating Context

Uso diário durante a reprodução de música. A Workbench reúne Now Playing, Search, Playlist, Lyrics e Editor; teclas e mouse controlam navegação e playback. O usuário confirmou preservar todas as telas e interações nesta tarefa visual.

## Capabilities and Constraints

Player usa Spotify via `librespot`; reprodução requer Spotify Premium. A interface é uma TUI Rust. Este redesign deve alterar somente apresentação visual, sem mudar comandos, atalhos, telas, conteúdo nem comportamento.

## Brand Commitments

Nome Riff e playlists `.riff` como texto versionável. Para o redesign atual, referências visuais fornecidas: configuração do MPD e tema/layout customizados de rmpc em `Sin-cy/dotfiles`.

## Evidence on Hand

- `README.md`: fluxos CLI, telas da Workbench e atalhos.
- `src/workbench/`: interface e comportamento existentes.
- As URLs fornecidas apontam a configuração MPD e o tema/layout customizados de rmpc.

## Product Principles

- Terminal first; TUI é a principal superfície.
- Texto versionável é a fonte da verdade das playlists.
- Preservar controles e comportamento durante mudanças exclusivamente visuais.
