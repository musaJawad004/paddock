// Types the Homebrew install into the hero terminal, switches install tabs,
// and copies code blocks. No requests, no tracking.
"use strict";

const LINES = [
  ["$ ", "brew tap musaJawad004/paddock https://github.com/musaJawad004/paddock"],
  ["$ ", "brew trust musaJawad004/paddock"],
  ["$ ", "brew install paddock"],
  ["", "==> paddock 0.2.0 installed"],
  ["$ ", "paddock"],
];

function typeInstall() {
  const out = document.getElementById("typed");
  if (!out) return;
  const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const render = (done, current) => {
    out.textContent = "";
    for (const [prompt, text] of done) addLine(prompt, text);
    if (current) addLine(current[0], current[1], true);
  };
  const addLine = (prompt, text, typing) => {
    const line = document.createElement("div");
    if (prompt) {
      const p = document.createElement("span");
      p.className = "prompt";
      p.textContent = prompt;
      line.append(p);
    }
    const t = document.createElement("span");
    t.textContent = text;
    if (!prompt) t.className = "ok";
    line.append(t);
    if (typing) {
      const c = document.createElement("span");
      c.className = "cursor";
      c.textContent = "█";
      line.append(c);
    }
    out.append(line);
  };
  if (reduced) {
    render(LINES, null);
    return;
  }
  let lineIndex = 0;
  let charIndex = 0;
  const done = [];
  const step = () => {
    if (lineIndex >= LINES.length) {
      setTimeout(() => {
        done.length = 0;
        lineIndex = 0;
        charIndex = 0;
        step();
      }, 4000);
      return;
    }
    const [prompt, text] = LINES[lineIndex];
    if (!prompt) {
      done.push([prompt, text]);
      lineIndex += 1;
      render(done, null);
      setTimeout(step, 700);
      return;
    }
    charIndex += 1;
    render(done, [prompt, text.slice(0, charIndex)]);
    if (charIndex >= text.length) {
      done.push([prompt, text]);
      lineIndex += 1;
      charIndex = 0;
      setTimeout(step, 600);
    } else {
      setTimeout(step, 28);
    }
  };
  step();
}

function tabs() {
  const buttons = document.querySelectorAll("[data-tab]");
  for (const button of buttons) {
    button.addEventListener("click", () => {
      for (const b of buttons) b.setAttribute("aria-selected", String(b === button));
      for (const panel of document.querySelectorAll("[data-panel]")) {
        panel.hidden = panel.dataset.panel !== button.dataset.tab;
      }
    });
  }
}

function copyButtons() {
  for (const button of document.querySelectorAll(".copy")) {
    button.addEventListener("click", async () => {
      const text = button.parentElement.querySelector("pre").textContent;
      try {
        await navigator.clipboard.writeText(text);
        button.textContent = "copied";
        button.classList.add("done");
      } catch {
        button.textContent = "select it";
      }
      setTimeout(() => {
        button.textContent = "copy";
        button.classList.remove("done");
      }, 1800);
    });
  }
}

typeInstall();
tabs();
copyButtons();
