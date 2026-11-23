import { useState } from 'react';
import type { editor } from 'monaco-editor';
import {
    type ResolvedControls,
    extractControls,
    resolveControls,
} from '../../dsl/extractControls';
import type { CodeStyle } from '../../dsl/listEdits';
import electronAPI from '../../electronAPI';
import { applyButtonChange, applyGroupCollapse } from '../buttonChange';
import { bindControls } from '../controlBinding';
import type { RunningPatchUi } from '../patchUi';
import { applySliderChange } from '../sliderChange';

const NO_CONTROLS: ResolvedControls = { buttons: [], groups: [], sliders: [] };

interface ControlState {
    /** The buffer and code `controls` were resolved from. */
    bufferId: string | undefined;
    code: string;
    controls: ResolvedControls;
    /** Last resolved controls per buffer, so a control whose arguments are
     *  mid-edit keeps showing its last valid state instead of vanishing. */
    byBuffer: ReadonlyMap<string, ResolvedControls>;
}

function resolveState(
    byBuffer: ReadonlyMap<string, ResolvedControls>,
    bufferId: string | undefined,
    code: string,
): ControlState {
    const controls = resolveControls(
        extractControls(code),
        bufferId === undefined ? undefined : byBuffer.get(bufferId),
    );
    return {
        bufferId,
        byBuffer:
            bufferId === undefined
                ? byBuffer
                : new Map(byBuffer).set(bufferId, controls),
        code,
        controls,
    };
}

const setModuleParam = (
    moduleId: string,
    moduleType: string,
    params: Record<string, unknown>,
) => {
    void electronAPI.synthesizer.setModuleParam(moduleId, moduleType, params);
};

/**
 * The visible buffer's controls, parsed from its code and bound to the
 * running patch's controls while the visible buffer is the running one.
 * Parsing is synchronous, so parsed call offsets always address the live
 * editor document.
 */
export function useControlViews(
    bufferId: string | undefined,
    code: string,
    patchUi: RunningPatchUi,
) {
    const [state, setState] = useState(() =>
        resolveState(new Map(), bufferId, code),
    );
    let current = state;
    if (state.bufferId !== bufferId || state.code !== code) {
        current = resolveState(state.byBuffer, bufferId, code);
        setState(current);
    }

    /** `controls` of buffer `ofBuffer`, bound to the running patch's when
     *  that buffer is the one it was evaluated from. */
    const bind = (controls: ResolvedControls, ofBuffer: string | undefined) =>
        bindControls(
            controls,
            patchUi.sliderDefs,
            patchUi.buttonDefs,
            ofBuffer !== undefined && ofBuffer === patchUi.sourceId,
            (moduleId) => patchUi.controlAnchors?.offsetOf(moduleId) ?? null,
        );

    return {
        /** The bound controls of another buffer, as last resolved. */
        bindBuffer: (ofBuffer: string) =>
            bind(current.byBuffer.get(ofBuffer) ?? NO_CONTROLS, ofBuffer),
        groups: current.controls.groups,
        views: bind(current.controls, bufferId),
    };
}

/**
 * What the control panel's gestures do: write the value into the visible
 * buffer's code and, for a bound control, into the running patch.
 */
export function useControlActions({
    views,
    editorInstance,
    codeStyle,
    onUse,
    setSliderValue,
    setButtonValue,
}: {
    views: ReturnType<typeof bindControls>;
    editorInstance: editor.IStandaloneCodeEditor | null;
    /** How generated code is laid out. */
    codeStyle: CodeStyle;
    /** A control was used; `offset` is where its arguments start. */
    onUse: (offset: number) => void;
    setSliderValue: (moduleId: string, value: number) => void;
    setButtonValue: (moduleId: string, value: boolean) => void;
}) {
    // Control offsets come from parsing the visible buffer, which is the
    // editor's current model.
    const model = () => editorInstance?.getModel() ?? null;

    const changeSlider = (callStart: number, newValue: number) => {
        const slider = views.sliders.find((s) => s.callStart === callStart);
        if (!slider) {
            return;
        }
        onUse(slider.argsStart);
        const written = applySliderChange(
            slider,
            newValue,
            slider.moduleId,
            model(),
            setModuleParam,
        );
        if (slider.moduleId !== null) {
            setSliderValue(slider.moduleId, written);
        }
    };

    const changeButton = (callStart: number, pressed: boolean) => {
        const button = views.buttons.find((b) => b.callStart === callStart);
        if (!button) {
            return;
        }
        if (pressed || button.mode === 'toggle') {
            onUse(button.argsStart);
        }
        applyButtonChange(
            button,
            pressed,
            button.moduleId,
            model(),
            setModuleParam,
        );
        if (button.moduleId !== null) {
            setButtonValue(button.moduleId, pressed);
        }
    };

    const toggleGroup = (callStart: number, collapsed: boolean) => {
        applyGroupCollapse({ callStart }, collapsed, model(), codeStyle);
    };

    const jumpTo = (offset: number) => {
        const textModel = model();
        if (!editorInstance || !textModel) {
            return;
        }
        const position = textModel.getPositionAt(offset);
        editorInstance.setPosition(position);
        editorInstance.revealPositionInCenterIfOutsideViewport(position);
        editorInstance.focus();
    };

    return { changeButton, changeSlider, jumpTo, toggleGroup };
}
