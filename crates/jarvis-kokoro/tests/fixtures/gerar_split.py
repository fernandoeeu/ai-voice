"""Gera split_text.json: copia fiel de split_text (tts/server.py), sem carregar o modelo.
Uso: python3 -I gerar_split.py > split_text.json
Devolve a lista de pedacos (antes do "\n".join) para que pedacos vazios fiquem visiveis."""
import json
import re

MAX_CHUNK = 250


def split_pieces(text: str) -> list:
    text = re.sub(r"[ \t\r\f\v]+", " ", text).strip()
    pieces = []
    for line in re.split(r"\n+", text):
        for sent in re.split(r"(?<=[.!?…;:])\s+", line.strip()):
            sent = sent.strip()
            while len(sent) > MAX_CHUNK:
                cut = max(sent.rfind(", ", 0, MAX_CHUNK), sent.rfind(" ", 0, MAX_CHUNK))
                if cut <= 0:
                    cut = MAX_CHUNK
                pieces.append(sent[:cut].strip(" ,"))
                sent = sent[cut:].strip(" ,")
            if sent:
                pieces.append(sent)
    return pieces


longo_virgulas = ", ".join(f"item numero {i} da lista" for i in range(1, 30))
longo_sem_pontuacao = " ".join(["palavra"] * 80)
sem_espaco = "a" * 600
sem_espaco_acentos = "ção" * 120
virgula_cedo = "Sim, " + "x" * 300
espacos_e_virgulas = ("lorem ipsum, " * 40).strip()
exato_250 = ("abcd " * 50).strip()
de_251 = ("abcd " * 50).strip() + "e"
virgula_na_borda = "a" * 247 + ", b" + " resto do texto " * 5

textos = [
    "Olá, tudo bem?",
    "Pronto. Fiz o deploy! Quer que eu confira os logs?",
    "Deixa eu ver… acho que sim. Talvez não; vamos testar: funciona?",
    "Primeiro passo: abrir o terminal; segundo passo: rodar o comando. Terceiro: conferir.",
    "Linha um.\nLinha dois.\n\n\nLinha três sem ponto",
    "  muitos    espaços \t e\ttabs   aqui.   Fim.  ",
    "Lista:\n- maçã\n- banana\n- laranja\n",
    "1. Instalar. 2. Configurar. 3. Rodar!",
    "O valor é 3.14 e custa R$ 1.234,56. Ok?",
    "Reticências... no meio... e no fim…",
    "Reticências unicode…seguidas sem espaço… e depois com espaço. Pronto",
    "Dr. Silva chegou. Sra. Souza também.",
    "",
    "   ",
    ".",
    "Só uma palavra",
    "Pergunta?! Exclamação!? Mais?!! Fim.",
    "Texto com\r\nquebra windows\r\ne mais uma linha.",
    "Ponto e vírgula; sem espaço;depois. Dois-pontos:colados: separados.",
    "Acentuação: não, é, à, ç, ü. Também: pão, ação, coração.",
    "Emoji 😀 no meio. Outro 🎉 aqui! Fim 🚀",
    "Espaço não separável aqui. E ideográfico　depois. Fim.",
    longo_virgulas,
    longo_virgulas + ". Depois uma frase curta. " + longo_virgulas + "!",
    longo_sem_pontuacao,
    sem_espaco,
    sem_espaco_acentos,
    virgula_cedo,
    espacos_e_virgulas,
    exato_250,
    de_251,
    virgula_na_borda,
    "Parágrafo um bem comprido, com várias vírgulas, que segue, segue, segue e segue, "
    "sem nunca terminar, porque o autor gosta de frases longas, muito longas, enormes, "
    "daquelas que ninguém lê até o fim, mas que o sintetizador precisa tratar direito. Fim.\n"
    "Parágrafo dois curto.",
    ", , , vírgulas soltas , , ,",
    "Fim com vírgula, ",
    "A" * 249 + " B" * 3 + ". " + "C" * 10,
]

print(json.dumps([{"texto": t, "pedacos": split_pieces(t)} for t in textos], ensure_ascii=False, indent=1))
