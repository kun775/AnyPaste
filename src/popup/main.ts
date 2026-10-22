import { mount } from "svelte";
import "../lib/theme.css";
import Popup from "./Popup.svelte";

mount(Popup, { target: document.getElementById("app")! });
