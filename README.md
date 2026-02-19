# WallpaperChanger

> Gerenciador de papel de parede para Windows com suporte a múltiplos monitores, collage, fade-in e bandeja do sistema.

![Python](https://img.shields.io/badge/Python-3.11%2B-blue?logo=python)
![Platform](https://img.shields.io/badge/Platform-Windows%2011-0078D4?logo=windows)
![License](https://img.shields.io/badge/License-MIT-green)

---

## Recursos

| Recurso | Descrição |
|---|---|
| **1 Imagem** | Uma única imagem cobre todo o desktop virtual (span) |
| **Collage** | Grade automática com 1 a 8 imagens por monitor |
| **Mesmas imagens em todos os monitores** | Opção dentro do Collage para replicar o mesmo conjunto em cada tela |
| **Seleção aleatória ou sequencial** | Alterna entre imagens de forma aleatória ou em ordem (mais recente → mais antiga) |
| **Ajuste de imagem** | Preencher, Ajustar, Ampliar, Centralizar ou Estender |
| **Rotação automática** | Troca o wallpaper em intervalos configuráveis (segundos) |
| **Efeito Fade-in** | Transição suave ao aplicar o novo wallpaper |
| **Minimizar para a bandeja** | O app continua rodando na área de notificação mesmo com a janela fechada |
| **CLI** | Controle total via linha de comando para uso em scripts e agendamento |

---

## Pré-requisitos

| Ferramenta | Versão mínima | Link |
|---|---|---|
| Windows | 10 / 11 | — |
| Python | 3.11+ | https://python.org |
| [uv](https://docs.astral.sh/uv/) | 0.4+ | https://docs.astral.sh/uv/ |

> **uv** é o gerenciador de pacotes/ambientes utilizado neste projeto. Instale com:
> ```powershell
> powershell -c "irm https://astral.sh/uv/install.ps1 | iex"
> ```

---

## Instalação (modo desenvolvimento)

```powershell
# 1. Clone o repositório
git clone https://github.com/klysman08/wallpaper-changer-windows.git
cd wallpaper-changer-windows/wallpaper-changer

# 2. Crie o ambiente virtual e instale as dependências
uv sync

# 3. Inicie a interface gráfica
uv run wallpaper-changer-gui
```

---

## Interface Gráfica (GUI)

Abra o painel de controle:

```powershell
uv run wallpaper-changer-gui
```

### Aba "Configurar"

**Modo de imagem**

- **1 Imagem** — uma imagem em span cobrindo todos os monitores.
- **Collage** — divide cada monitor em grade automática.
  - Escolha de **1 a 8 imagens por monitor** com os botões numéricos.
  - Ative **"Mesmas imagens em todos os monitores"** para replicar o mesmo conjunto em cada tela.

**Seleção de imagens** — `Aleatório` ou `Sequencial (recente → antigo)`.

**Ajuste na tela** — `Preencher`, `Ajustar`, `Ampliar`, `Centralizar`, `Estender`.

**Rotação automática** — defina o intervalo em segundos; clique em **Iniciar Watch** para ativar.

**Efeito Fade-in** — transição com 4 frames intermediários (~280 ms) ao trocar o wallpaper.

### Aba "Pasta"

Defina a pasta de origem das imagens (padrão: `C:\Users\Public\Pictures`).  
Formatos suportados: `jpg`, `jpeg`, `png`, `bmp`, `webp`.

### Barra de ações

| Botão | Ação |
|---|---|
| **Aplicar Agora** | Aplica o wallpaper imediatamente |
| **Salvar Config** | Persiste as configurações em `config/settings.toml` |
| **Iniciar / Parar Watch** | Ativa/desativa a rotação automática |
| **⬇ Bandeja** | Oculta a janela e mantém o app na bandeja do sistema |

### Bandeja do sistema

Clicar em fechar (✕) ou no botão **⬇ Bandeja** minimiza o app para a área de notificação.  
O ícone na bandeja oferece as opções: **Mostrar**, **Aplicar Agora** e **Sair**.

---

## CLI (Linha de Comando)

### Aplicar wallpaper imediatamente

```powershell
# Modo padrão (configurações de settings.toml)
uv run wallpaper-changer apply

# Modo 1 imagem, seleção aleatória
uv run wallpaper-changer apply --mode split1

# Modo collage com 4 imagens por monitor
uv run wallpaper-changer apply --mode collage --collage-count 4

# Seleção sequencial
uv run wallpaper-changer apply --selection sequential
```

### Rotação automática (watch)

```powershell
# Troca no intervalo definido em settings.toml (padrão: 300 s)
uv run wallpaper-changer watch
```

### Ajuda

```powershell
uv run wallpaper-changer --help
uv run wallpaper-changer apply --help
```

---

## Configuração (`config/settings.toml`)

```toml
[general]
mode                 = "split1"      # split1 | collage
selection            = "random"      # random | sequential
interval             = 300           # segundos entre trocas (watch)
collage_count        = 4             # imagens por monitor no collage (1-8)
collage_same_for_all = false         # mesmas imagens em todos os monitores
fade_in              = false         # efeito de transição suave

[paths]
wallpapers_folder = "C:\\Users\\Public\\Pictures"
output_folder     = "assets/output"

[display]
fit_mode = "fill"   # fill | fit | stretch | center | span
```

---

## Build — Gerando o executável `.exe`

O projeto inclui um script PowerShell que automatiza todo o processo com **PyInstaller**.

### Passos

```powershell
# A partir da pasta wallpaper-changer/
cd wallpaper-changer
.\scripts\build_exe.ps1
```

O script executa 5 etapas:

| Etapa | O que faz |
|---|---|
| 1 | `uv sync --dev` — sincroniza dependências incluindo PyInstaller |
| 2 | Remove as pastas `build/` e `dist/` de compilações anteriores |
| 3 | Executa `PyInstaller` com o arquivo `wallpaper_changer.spec` |
| 4 | Cria as pastas de runtime necessárias dentro de `dist/WallpaperChanger/` |
| 5 | Exibe o caminho e tamanho do executável gerado |

### Resultado

```
dist/
└── WallpaperChanger/
    ├── WallpaperChanger.exe   ← executável principal
    ├── config/
    │   └── settings.toml
    └── assets/
        ├── wallpapers/        ← coloque suas imagens aqui (ou use C:\Users\Public\Pictures)
        └── output/            ← wallpapers compostos (temporários)
```

> Para distribuir, copie **toda a pasta** `dist/WallpaperChanger/` para o destino.  
> O executável **não** requer Python instalado na máquina de destino.

### Build manual (sem o script)

```powershell
# Dentro da pasta wallpaper-changer/
uv sync --dev
uv run pyinstaller wallpaper_changer.spec --noconfirm
```

---

## Estrutura do projeto

```
wallpaper-changer/
├── main.py                        # entry point do PyInstaller
├── pyproject.toml                 # dependências e configuração do projeto
├── wallpaper_changer.spec         # spec do PyInstaller
├── config/
│   ├── settings.toml              # configurações persistidas
│   └── state.json                 # estado do modo sequencial
├── scripts/
│   └── build_exe.ps1              # script de build automatizado
└── src/
    └── wallpaper_changer/
        ├── gui.py                 # interface gráfica (CustomTkinter)
        ├── cli.py                 # interface de linha de comando (Click)
        ├── wallpaper.py           # lógica de composição e aplicação
        ├── image_utils.py         # seleção, redimensionamento e listagem
        ├── monitor.py             # detecção de monitores (screeninfo)
        └── config.py              # leitura e escrita do settings.toml
```

---

## Dependências principais

| Pacote | Uso |
|---|---|
| [CustomTkinter](https://github.com/TomSchimansky/CustomTkinter) | Interface gráfica moderna |
| [Pillow](https://python-pillow.org/) | Composição e redimensionamento de imagens |
| [pystray](https://github.com/moses-palmer/pystray) | Ícone na bandeja do sistema |
| [screeninfo](https://github.com/rr-/screeninfo) | Detecção de monitores e resoluções |
| [schedule](https://github.com/dbader/schedule) | Agendamento da rotação automática |
| [click](https://click.palletsprojects.com/) | Interface de linha de comando |
| [pywin32](https://github.com/mhammond/pywin32) | APIs do Windows |

---

## Licença

MIT — veja o arquivo [LICENSE](LICENSE) para detalhes.

