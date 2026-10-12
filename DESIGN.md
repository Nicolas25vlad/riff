---
name: Riff
description: Terminal-first player Spotify com playlists em texto.
colors:
  background: "#1E1E2E"
  foreground: "#E0DEF4"
  accent: "#FAB387"
  highlight: "#89B4FA"
  playback: "#9CCFD8"
  muted: "#908CAA"
  border: "#6E6A86"
  selection: "#26233A"
  danger: "#EB6F92"
---

# Design System: Riff

## Overview

**Creative North Star: "Faixa em primeiro plano"**

A Workbench apresenta primeiro a faixa e o estado do player, sem esconder arquivo ativo, navegação ou controles. Sua linguagem é uma TUI densa, organizada em linhas e áreas retangulares; a cor marca papéis de playback e seleção, não decoração.

**Key Characteristics:**
- Cabeçalho permanente para arquivo, faixa, artista e estado.
- Navegação por cinco vistas com seleção explícita.
- Capa e metadados dividem Now Playing em terminais amplos.
- Barra de progresso linear e controles persistentes.

## Colors

A base escura e lavanda do tema Catppuccin recebe pêssego, azul e aqua para separar controles, seleção e playback.

### Primary
- **Pêssego:** identifica controles ativos, títulos de painel e progresso.

### Secondary
- **Azul claro:** identifica seleção atual em resultados e fila.

### Tertiary
- **Aqua:** identifica o estado em reprodução e a linha ativa de letra.

### Neutral
- **Noite violeta:** fundo global da Workbench.
- **Lavanda clara:** texto principal.
- **Lavanda acinzentada:** texto auxiliar e instruções.
- **Borda violeta:** contornos de painéis e trilha vazia de progresso.
- **Violeta profundo:** fundo de seleções e controles.
- **Rosa:** erros de busca.

## Typography

A TUI herda a fonte e o tamanho de célula do emulador de terminal. O Riff distingue informação com peso em negrito, alinhamento e cor; não define fontes nem tamanhos tipográficos.

## Layout

O shell mantém duas linhas de cabeçalho, duas linhas de abas, a vista ativa, duas linhas de transporte/progresso e uma linha de atalhos. As cinco abas dividem a largura igualmente. Now Playing usa capa e metadados lado a lado a partir de 82 colunas por 14 linhas; em terminais estreitos, a capa empilha acima dos metadados quando há pelo menos 20 linhas. O transporte usa rótulos compactos abaixo de 72 colunas.

## Elevation & Depth

A interface é plana; profundidade vem de áreas delimitadas por bordas, diferença tonal de fundo e estado selecionado. Não usa sombras.

## Shapes

Blocos e contornos seguem a geometria retangular da célula de terminal. Painéis usam bordas simples; a seleção usa fundo tonal, não cápsulas ou cartões arredondados.

## Components

### Cabeçalho
- Duas linhas mantêm identidade/arquivo, faixa, artista, status, tempo, volume e modos visíveis.
- Estado em reprodução usa aqua; outros estados usam o acento principal.

### Navegação
- Cinco abas de mesma largura; a vista ativa usa texto em negrito sobre fundo de seleção.
- Navegação continua controlada pelos atalhos e hitboxes já existentes.

### Busca e fila
- Item selecionado recebe texto azul claro e fundo violeta profundo.
- Títulos de painel usam pêssego; erros de busca usam rosa.

### Transporte
- Controles anteriores, play/pause e próximos têm fundo tonal; play/pause diferencia o estado em reprodução.
- Volume continua como gauge; progresso usa linha pesada preenchida e linha pontilhada vazia.

## Do's and Don'ts

### Do:
- **Do** usar os papéis cromáticos declarados no frontmatter para a seleção, o playback e os erros.
- **Do** manter faixa, estado e tempo visíveis no shell da Workbench.
- **Do** preservar layout adaptável a terminais estreitos.

### Don't:
- **Don't** alterar atalhos, hitboxes, semântica das vistas ou ações ao mudar a aparência.
- **Don't** substituir o texto da playlist por estado opaco de aplicativo.
- **Don't** aplicar gradientes, sombras ou formas que não existem na TUI.
