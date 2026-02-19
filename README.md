# WallpaperChanger

> Gerenciador de papel de parede collage para Windows com suporte a múltiplos monitores, fade-in e bandeja do sistema.

![Python](https://img.shields.io/badge/Python-3.11%2B-blue?logo=python)
![Platform](https://img.shields.io/badge/Platform-Windows%2011-0078D4?logo=windows)
![License](https://img.shields.io/badge/License-MIT-green)

---

## Recursos

| Recurso | Descrição |
|---|---|
| **Collage** | Grade automática com 1 a 8 imagens por monitor |
| **Mesmas imagens em todos os monitores** | Opção para replicar o mesmo conjunto em cada tela |
| **Seleção aleatória ou sequencial** | Alterna entre imagens de forma aleatória ou em ordem |
| **Ajuste de imagem** | Preencher, Ajustar, Ampliar, Centralizar ou Estender |
| **Rotação automática** | Troca o wallpaper em intervalos configuráveis |
| **Efeito Fade** | Transição suave ao aplicar o novo wallpaper |
| **Iniciar com o Windows** | Opção para executar automaticamente ao ligar o PC |
| **Minimizar para a bandeja** | O app continua rodando na área de notificação |
| **Instalador Windows** | Setup.exe via Inno Setup para instalação fácil |
| **CLI** | Controle via linha de comando |

---

## Pré-requisitos

| Ferramenta | Versão mínima | Link |
|---|---|---|
| Windows | 10 / 11 | — |
| Python | 3.11+ | https://python.org |
| [uv](https://docs.astral.sh/uv/) | 0.4+ | https://docs.astral.sh/uv/ |

---

## Instalação (modo desenvolvimento)

```powershell
# 1. Clone o repositório
git clone https://github.com/klysman08/wallpaper-changer-windows.git
cd wallpaper-changer-windows/wallpaper-changer

# 2. Crie o ambiente virtual e instale as dependências
uv sync

# 3. Inicie a interface gráfica
uv run python -c "from wallpaper_changer.gui import run; run()"
```

---

## Instalação (via instalador)

1. Baixe o `WallpaperChanger_Setup.exe` da página de releases
2. Execute o instalador e siga as instruções
3. Opcionalmente marque "Iniciar com o Windows" durante a instalação

---

## Interface Gráfica (GUI)

### Collage

Cada monitor é dividido em uma grade automática com **1 a 8 imagens**.

- Escolha o número de imagens com os botões numéricos
- Ative **"Mesmas imagens em todos os monitores"** para replicar o mesmo conjunto

### Configurações

- **Seleção de imagens** — `Aleatório` ou `Sequencial`
- **Ajuste na tela** — `Preencher`, `Ajustar`, `Ampliar`, `Centralizar`, `Estender`
- **Rotação automática** — defina o intervalo em segundos e clique em **Iniciar Watch**
- **Efeito Fade** — transição suave com 8 frames intermediários ao trocar o wallpaper
- **Iniciar com o Windows** — registra o app para executar automaticamente no login

### Pasta de Wallpapers

Defina a pasta de origem das imagens.
Formatos suportados: `jpg`, `jpeg`, `png`, `bmp`, `webp`.

### Bandeja do sistema

Fechar (✕) minimiza para a bandeja. Opções: **Mostrar**, **Aplicar Agora**, **Sair**.

---

## CLI

```powershell
# Aplicar wallpaper imediatamente
uv run wallpaper-changer apply

# Aplicar com opções
uv run wallpaper-changer apply --collage-count 6 --selection random

# Modo watch (troca automática)
uv run wallpaper-changer watch
```

---

## Build

### Executável portável (PyInstaller)

```powershell
cd wallpaper-changer
.\scripts\build_exe.ps1 -NoInstaller
```

Resultado em `dist\WallpaperChanger\`.

### Instalador Windows (Inno Setup)

Pré-requisito: [Inno Setup 6](https://jrsoftware.org/isdl.php) instalado.

```powershell
cd wallpaper-changer
.\scripts\build_exe.ps1
```

Resultado: `dist\WallpaperChanger_Setup.exe`.

---

## Estrutura

```
wallpaper-changer/
├── main.py                  # Entry point PyInstaller
├── pyproject.toml           # Dependências e metadados
├── wallpaper_changer.spec   # Spec do PyInstaller
├── installer.iss            # Script do Inno Setup
├── config/
│   └── settings.toml        # Configurações do app
├── scripts/
│   └── build_exe.ps1        # Script de build
└── src/wallpaper_changer/
    ├── __init__.py
    ├── cli.py               # Interface de linha de comando
    ├── config.py            # Leitura/escrita de configurações
    ├── gui.py               # Interface gráfica (ttkbootstrap)
    ├── image_utils.py       # Seleção e redimensionamento de imagens
    ├── monitor.py           # Detecção de monitores
    ├── startup.py           # Inicialização com o Windows
    └── wallpaper.py         # Montagem e aplicação do wallpaper
```
