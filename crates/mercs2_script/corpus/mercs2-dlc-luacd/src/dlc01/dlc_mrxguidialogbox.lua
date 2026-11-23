local L0_1, L1_1, L2_1, L3_1
import("MrxGuiBase", false)
_ksFont = "english_18"
_knScale = 1
_knTextR = 156
_knTextG = 154
_knTextB = 133
_knTextLitR = 210
_knTextLitG = 210
_knTextLitB = 190
_ksAcceptSound = "ui_PDA_Accept"
_ksCancelSound = "ui_PDA_Cancel"
_ksChangeSound = "ui_PDA_Scroll"

function L0_1(A0_2, A1_2, A2_2, A3_2, A4_2, A5_2, A6_2, A7_2, A8_2, A9_2, A10_2, A11_2)
  local L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2
  L12_2 = type(A1_2)
  if "string" ~= L12_2 then
    return
  end
  L12_2 = type(A2_2)
  if "table" ~= L12_2 then
    return
  end
  L12_2 = table.getn(A2_2)
  if L12_2 < 1 then
    A2_2[1] = "[Generic.Ok]"
  end
  A3_2 = _ValidateParameter(A3_2, "number", 1)
  A4_2 = _ValidateParameter(A4_2, "function", nil)
  A5_2 = _ValidateParameter(A5_2, "table", {})
  A11_2 = _ValidateParameter(A11_2, "number", nil)
  if nil == A10_2 then
    A10_2 = true
  end
  L12_2 = _BuildDialogBox(A1_2, A2_2, A3_2, A4_2, A5_2, A0_2, A6_2, A7_2, A8_2, A9_2, A11_2)
  L12_2.SetOwner(L12_2, A0_2)
  MrxGuiBase.GetControlFocus(L12_2, A10_2)
  L12_2.Close = Close
  return L12_2
end

DisplayDialogBox = L0_1

function L0_1(A0_2)
  local L1_2, L2_2
  MrxGuiBase.ReleaseControlFocus(A0_2)
  MrxGuiBase.RemoveWidgetWithChildren(A0_2)
  A0_2.DeleteWithChildren(A0_2)
end

Close = L0_1

function L0_1(A0_2, A1_2, A2_2, A3_2, A4_2, A5_2, A6_2, A7_2, A8_2, A9_2, A10_2)
  local L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2, L29_2, L30_2, L31_2, L32_2, L33_2, L34_2, L35_2, L36_2, L37_2, L38_2, L39_2, L40_2, L41_2, L42_2, L43_2, L44_2, L45_2, L46_2, L47_2, L48_2, L49_2, L50_2, L51_2
  L11_2 = 170
  L12_2 = 100
  L13_2 = 298
  L14_2 = 10
  L15_2 = 20
  L17_2 = MrxGuiBase.ImageWidget
  L17_2 = L17_2.new(L17_2)
  L17_2.SetLocation(L17_2, L11_2, L12_2, (L11_2 + L13_2), 400)
  L18_2 = L17_2.BasicData
  L18_2.bContainer = true
  L17_2.SetOwner(L17_2, A5_2)
  L17_2.SetVisible(L17_2, false)
  L18_2 = MrxGuiBase.TextWidget
  L18_2 = L18_2.new(L18_2)
  L18_2.SetLocation(L18_2, (L11_2 + L14_2), (L12_2 + L15_2), ((L11_2 + L13_2) - L14_2), 390)
  L18_2.SetFont(L18_2, _ksFont)
  L18_2.SetScale(L18_2, _knScale)
  L18_2.SetColor(L18_2, _knTextLitR, _knTextLitG, _knTextLitB)
  L18_2.SetText(L18_2, A0_2)
  L18_2.Wrap(L18_2)
  L18_2.SetOwner(L18_2, A5_2)
  L18_2.ParentWidget = L17_2
  L16_2 = ((L15_2 + L12_2) + L18_2.GetHeight(L18_2)) + 4
  L19_2 = MrxGuiBase.ImageWidget
  L19_2 = L19_2.new(L19_2)
  L19_2.SetColor(L19_2, 84, 79, 57)
  L19_2.SetTranslucency(L19_2, 205)
  L19_2.SetLocation(L19_2, (L11_2 + L14_2), 1, ((L11_2 + L13_2) - L14_2), 2)
  L19_2.SetOwner(L19_2, A5_2)
  L19_2.ParentWidget = L17_2
  L20_2 = L19_2.CustomData
  L23_2 = {}
  L23_2.y = 1
  L23_2.y2 = 1
  L20_2.nClosePoint = L19_2.AddAnimationPoint(L19_2, L23_2)
  L20_2 = L19_2.CustomData
  L23_2 = {}
  L23_2.y = 1
  L23_2.y2 = 1
  L20_2.nOpenPoint = L19_2.AddAnimationPoint(L19_2, L23_2)
  L20_2 = L19_2.CustomData
  L23_2 = {}
  L23_2.TranslucencyLevel = 255
  L20_2.nPulseHighPoint = L19_2.AddAnimationPoint(L19_2, L23_2)
  L20_2 = L19_2.CustomData
  L23_2 = {}
  L23_2.TranslucencyLevel = 100
  L20_2.nPulseLowPoint = L19_2.AddAnimationPoint(L19_2, L23_2)
  L19_2.SetIgnoresPause(L19_2, true)
  L20_2 = MrxGuiBase.ImageWidget
  L20_2 = L20_2.new(L20_2)
  L20_2.SetTranslucency(L20_2, 0)
  L20_2.SetLocation(L20_2, (L11_2 + L14_2), L16_2, ((L11_2 + L13_2) - L14_2), (L16_2 + 200))
  L20_2.SetOwner(L20_2, A5_2)
  L20_2.ParentWidget = L17_2
  L21_2 = 50
  L22_2 = 1
  while "string" do
    L23_2 = type(A1_2[L22_2])
    if "string" ~= L23_2 then
      break
    end
    L23_2 = MrxGuiBase.TextWidget
    L23_2 = L23_2.new(L23_2)
    L23_2.SetLocation(L23_2, (L11_2 + L21_2), L16_2, ((L11_2 + L13_2) - L14_2), L16_2)
    L23_2.SetFont(L23_2, _ksFont)
    L23_2.SetScale(L23_2, _knScale)
    L23_2.SetColor(L23_2, _knTextR, _knTextG, _knTextB)
    L23_2.SetText(L23_2, A1_2[L22_2])
    L23_2.Wrap(L23_2)
    L23_2.SetOwner(L23_2, A5_2)
    L23_2.ParentWidget = L20_2
    L20_2.AddChild(L20_2, L23_2)
    L24_2 = L23_2.CustomData
    L24_2.nHeight = L23_2.GetHeight(L23_2)
    L22_2 = L22_2 + 1
    L16_2 = (L16_2 + L23_2.CustomData.nHeight) + 2
  end
  if 1 == L22_2 then
    L17_2.DeleteWithChildren(L17_2)
    L23_2 = nil
    return L23_2
  end
  L23_2 = L20_2.GetChildren(L20_2)[A2_2]
  if not L23_2 then
    A2_2 = 1
  end
  L23_2 = L17_2.CustomData
  L23_2.nSelectedIndex = A2_2
  L23_2 = L20_2.GetChildren(L20_2)[L17_2.CustomData.nSelectedIndex]
  L25_2 = L23_2
  L24_2 = L23_2.GetLocation
  L24_2, L25_2 = L24_2(L25_2)
  L19_2.SetLocation(L19_2, nil, L25_2, nil, (L25_2 + L23_2.CustomData.nHeight))
  L23_2.SetColor(L23_2, _knTextLitR, _knTextLitG, _knTextLitB)
  L26_2 = MrxGuiBase.TextWidget
  L26_2 = L26_2.new(L26_2)
  L16_2 = L16_2 + 8
  L26_2.SetLocation(L26_2, (L11_2 + L14_2), L16_2, ((L11_2 + L13_2) - L14_2), L16_2)
  L26_2.SetFont(L26_2, _ksFont)
  L26_2.SetScale(L26_2, _knScale)
  L26_2.SetColor(L26_2, _knTextR, _knTextG, _knTextB)
  L26_2.SetJustification(L26_2, "center")
  L27_2 = #A1_2
  if 1 < L27_2 then
    L26_2.SetText(L26_2, "[move] [PDA.Common.MoveSelection]  [confirm] [Generic.Confirm]")
  else
    L26_2.SetText(L26_2, "[confirm] [Generic.Confirm]")
  end
  L26_2.SetOwner(L26_2, A5_2)
  L26_2.ParentWidget = L17_2
  L27_2 = L26_2.CustomData
  L27_2.nHeight = L26_2.GetHeight(L26_2)
  L16_2 = (L16_2 + L26_2.CustomData.nHeight) - 8
  L27_2 = {}
  L28_2 = 0
  L29_2 = 0.8730469
  L30_2 = 0
  L31_2 = 0.078125
  L34_2 = 0.1796875
  L35_2 = 0.2734375
  L36_2 = 0.6666667
  L37_2 = 447 * L36_2
  L38_2 = 48 * L36_2
  L39_2 = 46 * L36_2
  L40_2 = 48 * L36_2
  L42_2 = "global_gui_hud02"
  L43_2 = L12_2
  L44_2 = 2
  L45_2 = MrxGuiBase.ImageWidget
  L27_2[1] = L45_2.new(L45_2)
  L45_2 = L27_2[1]
  L45_2.SetTexture(L45_2, L42_2)
  L45_2 = L27_2[1]
  L45_2.SetLocation(L45_2, L11_2, L43_2, (L11_2 + L37_2), (L43_2 + L38_2))
  L45_2 = L27_2[1]
  L45_2.SetTextureCoordinates(L45_2, L28_2, L35_2, L29_2, L34_2)
  L45_2 = L27_2[1]
  L45_2.SetOwner(L45_2, A5_2)
  L41_2 = (L16_2 - L12_2) - L38_2
  L43_2 = L43_2 + L38_2
  L39_2 = (L41_2 - (L40_2 - (20 * L36_2))) + L15_2
  L45_2 = MrxGuiBase.ImageWidget
  L27_2[L44_2] = L45_2.new(L45_2)
  L45_2 = L27_2[L44_2]
  L45_2.SetTexture(L45_2, L42_2)
  L45_2 = L27_2[L44_2]
  L45_2.SetLocation(L45_2, L11_2, L43_2, (L11_2 + L37_2), (L43_2 + L39_2))
  L45_2 = L27_2[L44_2]
  L45_2.SetTextureCoordinates(L45_2, L28_2, 0.083984375, L29_2, 0.17382812)
  L45_2 = L27_2[L44_2]
  L45_2.SetOwner(L45_2, A5_2)
  L41_2 = L41_2 - L39_2
  L43_2 = L43_2 + L39_2
  L44_2 = L44_2 + 1
  L45_2 = MrxGuiBase.ImageWidget
  L27_2[L44_2] = L45_2.new(L45_2)
  L45_2 = L27_2[L44_2]
  L45_2.SetTexture(L45_2, L42_2)
  L45_2 = L27_2[L44_2]
  L45_2.SetLocation(L45_2, L11_2, L43_2, (L11_2 + L37_2), (L43_2 + L40_2))
  L45_2 = L27_2[L44_2]
  L45_2.SetTextureCoordinates(L45_2, L28_2, L34_2, L29_2, L35_2)
  L45_2 = L27_2[L44_2]
  L45_2.SetOwner(L45_2, A5_2)
  L43_2 = L43_2 + L40_2
  L44_2 = 1
  while true do
    L45_2 = L27_2[L44_2]
    if not L45_2 then
      break
    end
    L17_2.AddChild(L17_2, L27_2[L44_2])
    L44_2 = L44_2 + 1
  end
  L17_2.AddChild(L17_2, L18_2)
  L17_2.AddChild(L17_2, L19_2)
  L17_2.AddChild(L17_2, L20_2)
  L17_2.AddChild(L17_2, L26_2)
  L45_2 = L17_2.CustomData
  L45_2.oCursor = L19_2
  L45_2 = L17_2.CustomData
  L45_2.oOptions = L20_2
  L45_2 = L17_2.CustomData
  L45_2.fCallback = A3_2
  L45_2 = L17_2.CustomData
  L45_2.tCallbackArgs = A4_2
  L45_2 = L17_2.CustomData
  L45_2.nCancelOption = A10_2
  L12_2 = 240 - ((L43_2 - L12_2) / 2)
  if L12_2 < 0 then
    L12_2 = 0
  end
  L17_2.SetLocation(L17_2, L11_2, L12_2)
  L17_2.SetCoordinates(L17_2, L11_2, L12_2, (L11_2 + L13_2), (L12_2 + L45_2))
  if A6_2 and A7_2 and A8_2 and A9_2 then
    L41_2 = L45_2
    L46_2 = nil
    L47_2 = nil
    if "left" == A8_2 then
      if A6_2 < 48 then
        A6_2 = 48
      end
      L46_2 = A6_2
    elseif "right" == A8_2 then
      if A6_2 < 48 then
        A6_2 = 48
      end
      L46_2 = (640 - L13_2) - A6_2
    elseif "center" == A8_2 then
      L46_2 = (A6_2 - (L13_2 * 0.5)) + 320
      if L46_2 < 48 then
        L46_2 = 48
      else
        L48_2 = 592 - L13_2
        if L46_2 > L48_2 then
          L46_2 = 592 - L13_2
        end
      end
    end
    if "top" == A9_2 then
      if A7_2 < 36 then
        A7_2 = 36
      end
      L47_2 = A7_2
    elseif "bottom" == A9_2 then
      if A7_2 < 36 then
        A7_2 = 36
      end
      L47_2 = (480 - L41_2) - A7_2
    elseif "center" == A9_2 then
      L47_2 = (A7_2 - (L41_2 * 0.5)) + 240
      if L47_2 < 36 then
        L47_2 = 36
      else
        L48_2 = 444 - L41_2
        if L47_2 > L48_2 then
          L47_2 = 444 - L41_2
        end
      end
    end
    L17_2.SetAnchoring(L17_2, A8_2, A9_2)
    L17_2.SetLocation(L17_2, L46_2, L47_2)
  end
  MrxGuiBase.AddWidgetWithChildren(L17_2)
  L17_2.SetEventHandler(L17_2, "ControllerInput", _HandleInputEvent)
  L17_2._ChangeSelection = _ChangeSelection
  Pulse(L19_2)
  return L17_2
end

_BuildDialogBox = L0_1
_knPulseTime = 0.5

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L1_2 = A0_2.GetTranslucency(A0_2)
  L2_2 = A0_2.CustomData.bRising
  if L2_2 then
    A0_2.AnimateToPoint(A0_2, A0_2.CustomData.nPulseHighPoint, (((255 - L1_2) / 255) * _knPulseTime), true, _LoopToLow)
  else
    A0_2.AnimateToPoint(A0_2, A0_2.CustomData.nPulseLowPoint, (((L1_2 - 100) / 255) * _knPulseTime), true, _LoopToHigh)
  end
end

Pulse = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  L1_2 = A0_2.CustomData
  L1_2.bRising = true
  A0_2.AnimateToPoint(A0_2, A0_2.CustomData.nPulseHighPoint, _knPulseTime, true, _LoopToLow)
end

_LoopToHigh = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  L1_2 = A0_2.CustomData
  L1_2.bRising = false
  A0_2.AnimateToPoint(A0_2, A0_2.CustomData.nPulseLowPoint, _knPulseTime, true, _LoopToHigh)
end

_LoopToLow = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L1_2 = A0_2.CustomData
  L1_2.bRising = false
  L1_2 = bImmediate
  if L1_2 then
    A0_2.AnimateToPoint(A0_2, A0_2.CustomData.nPulseHighPoint, (((255 - A0_2.GetTranslucency(A0_2)) / 255) * _knPulseTime), true)
  else
    A0_2.AnimateToPoint(A0_2, A0_2.CustomData.nPulseHighPoint, 0, true)
  end
end

HaltPulse = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2
  L2_2 = 1
  if A1_2 then
    L2_2 = -1
  end
  L3_2 = A0_2.CustomData.oOptions
  L4_2 = L3_2.GetChildren(L3_2)[A0_2.CustomData.nSelectedIndex]
  L4_2.SetColor(L4_2, _knTextR, _knTextG, _knTextB)
  L5_2 = A0_2.CustomData
  L5_2.nSelectedIndex = (A0_2.CustomData.nSelectedIndex + L2_2)
  if A1_2 then
    L5_2 = A0_2.CustomData.nSelectedIndex
    if L5_2 < 1 then
      L5_2 = A0_2.CustomData
      L5_2.nSelectedIndex = #L3_2
    end
  else
    L5_2 = L3_2[A0_2.CustomData.nSelectedIndex]
    if not L5_2 then
      L5_2 = A0_2.CustomData
      L5_2.nSelectedIndex = 1
    end
  end
  L5_2 = L3_2[A0_2.CustomData.nSelectedIndex]
  L7_2 = L4_2
  L6_2 = L4_2.GetLocation
  L6_2, L7_2 = L6_2(L7_2)
  L10_2 = L5_2
  L9_2 = L5_2.GetLocation
  L9_2, L10_2 = L9_2(L10_2)
  L11_2 = L10_2 + L5_2.CustomData.nHeight
  L12_2 = A0_2.CustomData.oCursor
  L14_2 = L12_2
  L13_2 = L12_2.GetLocation
  L13_2, L14_2, L15_2, L16_2 = L13_2(L14_2)
  L17_2 = (L14_2 + L16_2) * 0.5
  L21_2 = {}
  L21_2.y = L17_2
  L21_2.y2 = L17_2
  L12_2.SetAnimationPoint(L12_2, L12_2.CustomData.nClosePoint, L21_2)
  L21_2 = {}
  L21_2.y = L10_2
  L21_2.y2 = L11_2
  L12_2.SetAnimationPoint(L12_2, L12_2.CustomData.nOpenPoint, L21_2)
  L12_2.SetLocation(L12_2, nil, L7_2, nil, (L7_2 + L4_2.CustomData.nHeight))
  L24_2 = {}
  L24_2[1] = L10_2
  L24_2[2] = L11_2
  L24_2[3] = L5_2
  L12_2.AnimateToPoint(L12_2, L12_2.CustomData.nClosePoint, 0.075, true, _CompleteAnimation, L24_2)
end

_ChangeSelection = L0_1

function L0_1(A0_2, A1_2, A2_2, A3_2)
  local L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L4_2 = (A1_2 + A2_2) * 0.5
  A0_2.SetLocation(A0_2, nil, L4_2, nil, L4_2)
  L8_2 = {}
  L8_2.y = A1_2
  L8_2.y2 = A2_2
  A0_2.SetAnimationPoint(A0_2, A0_2.CustomData.nOpenPoint, L8_2)
  A0_2.AnimateToPoint(A0_2, A0_2.CustomData.nOpenPoint, 0.075, true, Pulse)
  A3_2.SetColor(A3_2, _knTextLitR, _knTextLitG, _knTextLitB)
end

_CompleteAnimation = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2
  L2_2 = MrxGuiBase.Joystick.BUTTON_PAD1_D
  L3_2 = A1_2.ButtonPress
  if L2_2 ~= L3_2 then
    L2_2 = MrxGuiBase.Joystick.BUTTON_L_STICK_D
    L3_2 = A1_2.ButtonPress
    if L2_2 ~= L3_2 then
      goto lbl_22
    end
  end
  A0_2._ChangeSelection(A0_2, false)
  Sound.CueSound(0, _ksChangeSound)
  goto lbl_80
  ::lbl_22::
  L2_2 = MrxGuiBase.Joystick.BUTTON_PAD1_U
  L3_2 = A1_2.ButtonPress
  if L2_2 ~= L3_2 then
    L2_2 = MrxGuiBase.Joystick.BUTTON_L_STICK_U
    L3_2 = A1_2.ButtonPress
    if L2_2 ~= L3_2 then
      goto lbl_43
    end
  end
  A0_2._ChangeSelection(A0_2, true)
  Sound.CueSound(0, _ksChangeSound)
  goto lbl_80
  ::lbl_43::
  L2_2 = MrxGuiBase.Joystick.BUTTON_PAD2_D
  L3_2 = A1_2.ButtonPress
  if L2_2 == L3_2 then
    _CloseAndCallCallback(A0_2, A0_2.CustomData.nSelectedIndex)
    Sound.CueSound(0, _ksAcceptSound)
  else
    L2_2 = MrxGuiBase.Joystick.BUTTON_PAD2_R
    L3_2 = A1_2.ButtonPress
    if L2_2 == L3_2 then
      L2_2 = A0_2.CustomData.nCancelOption
      if L2_2 then
        _CloseAndCallCallback(A0_2, A0_2.CustomData.nCancelOption)
        Sound.CueSound(0, _ksCancelSound)
      end
    end
  end
  ::lbl_80::
end

_HandleInputEvent = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2
  MrxGuiBase.ReleaseControlFocus(A0_2)
  L2_2 = A0_2.CustomData.fCallback
  L3_2 = A0_2.CustomData.tCallbackArgs
  A0_2.Close(A0_2)
  if L2_2 then
    table.insert(L3_2, A1_2)
    L5_2 = unpack
    L6_2 = L3_2
    L5_2, L6_2 = L5_2(L6_2)
    L2_2(L5_2, L6_2)
  end
end

_CloseAndCallCallback = L0_1
L0_1 = 10
L1_1 = 200
L2_1 = 100

function L3_1(A0_2, A1_2, A2_2, A3_2, A4_2, A5_2, A6_2, A7_2)
  local L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2
  L8_2 = type(A0_2)
  if "userdata" ~= L8_2 then
    return
  end
  L8_2 = type(A1_2)
  if "string" ~= L8_2 then
    return
  end
  L11_2 = A4_2
  L12_2 = A2_2
  L8_2 = _BuildScrollingDialogBox(A0_2, A1_2, L11_2, L12_2, A3_2, A5_2, A6_2, A7_2)
  L10_2 = L8_2
  L9_2 = L8_2.GetLocation
  L9_2, L10_2, L11_2, L12_2 = L9_2(L10_2)
  L8_2.SetLocation(L8_2, (320 - ((L11_2 - L9_2) * 0.5)), (240 - ((L12_2 - L10_2) * 0.5)))
  L8_2.SetAnchoring(L8_2, "center", "center")
  MrxGuiBase.AddWidgetWithChildren(L8_2)
  MrxGuiBase.GetControlFocus(L8_2, false)
  return L8_2
end

DisplayScrollingDialogBox = L3_1

function L3_1(A0_2, A1_2, A2_2, A3_2, A4_2, A5_2, A6_2, A7_2)
  local L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2, L24_2, L25_2, L26_2, L27_2, L28_2, L29_2, L30_2, L31_2, L32_2, L33_2, L34_2, L35_2, L36_2, L37_2, L38_2, L39_2, L40_2
  L9_2 = 10
  L10_2 = 30
  L11_2 = MrxGuiBase.TextWidget
  L11_2 = L11_2.new(L11_2)
  L11_2.SetFont(L11_2, "english_18")
  L11_2.SetText(L11_2, A1_2)
  L11_2.SetLocation(L11_2, 0, 0, ((298 - L9_2) - L9_2), L1_1)
  L11_2.Wrap(L11_2)
  L11_2.SetOwner(L11_2, A0_2)
  L12_2 = nil
  L13_2 = L1_1
  L14_2 = L11_2.GetHeight(L11_2)
  L15_2 = L1_1
  if L14_2 > L15_2 then
    L12_2 = _CreateScrollableWindow(A0_2, L11_2, 0, 0, ((L8_2 - (L9_2 * 2)) - L0_1), L1_1)
    L11_2.delete(L11_2)
    L11_2 = nil
  else
    L13_2 = L11_2.GetHeight(L11_2)
  end
  L14_2 = MrxGuiBase.Widget
  L14_2 = L14_2.new(L14_2)
  L15_2 = 50
  L16_2 = 2
  if A2_2 then
    L16_2 = 3
  end
  L14_2.SetLocation(L14_2, 0, 0, (((L8_2 - (L9_2 * 2)) / L16_2) * L16_2), L15_2)
  L14_2.SetOwner(L14_2, A0_2)
  L18_2 = MrxGuiBase.TextWidget
  L18_2 = L18_2.new(L18_2)
  L19_2 = MrxGuiBase.TextWidget
  L19_2 = L19_2.new(L19_2)
  L22_2 = "english_18"
  L18_2.SetFont(L18_2, L22_2)
  L21_2 = L18_2
  L20_2 = L18_2.SetText
  L22_2 = A5_2 or L22_2
  if not A5_2 then
    L22_2 = "[Generic.Accept]"
  end
  L20_2(L21_2, L22_2)
  L18_2.SetOwner(L18_2, A0_2)
  L18_2.SetJustification(L18_2, "center")
  L20_2 = L18_2.GetHeight(L18_2)
  L18_2.SetLocation(L18_2, 0, ((L15_2 / 2) - (L20_2 / 2)), L17_2, ((L15_2 / 2) + (L20_2 / 2)))
  L18_2.Wrap(L18_2)
  L20_2 = L18_2.GetHeight(L18_2)
  L18_2.SetLocation(L18_2, 0, ((L15_2 / 2) - (L20_2 / 2)), L17_2, ((L15_2 / 2) + (L20_2 / 2)))
  _BuildStrokes(L18_2, 2, 0, (L17_2 - 2), L15_2)
  L23_2 = "english_18"
  L19_2.SetFont(L19_2, L23_2)
  L22_2 = L19_2
  L21_2 = L19_2.SetText
  L23_2 = A6_2 or L23_2
  if not A6_2 then
    L23_2 = "[Generic.Decline]"
  end
  L21_2(L22_2, L23_2)
  L19_2.SetOwner(L19_2, A0_2)
  L19_2.SetJustification(L19_2, "center")
  L19_2.SetLocation(L19_2, L17_2, ((L15_2 / 2) - (L20_2 / 2)), (L17_2 * 2), ((L15_2 / 2) + (L20_2 / 2)))
  L19_2.Wrap(L19_2)
  L20_2 = L19_2.GetHeight(L19_2)
  L19_2.SetLocation(L19_2, L17_2, ((L15_2 / 2) - (L20_2 / 2)), (L17_2 * 2), ((L15_2 / 2) + (L20_2 / 2)))
  _BuildStrokes(L19_2, (L17_2 + 2), 0, ((L17_2 * 2) - 2), L15_2)
  L21_2 = nil
  if A2_2 then
    L22_2 = MrxGuiBase.TextWidget
    L21_2 = L22_2.new(L22_2)
    L24_2 = "english_18"
    L21_2.SetFont(L21_2, L24_2)
    L23_2 = L21_2
    L22_2 = L21_2.SetText
    L24_2 = A7_2 or L24_2
    if not A7_2 then
      L24_2 = "[Briefing.ChangeWager]"
    end
    L22_2(L23_2, L24_2)
    L21_2.SetJustification(L21_2, "center")
    L21_2.SetOwner(L21_2, A0_2)
    L22_2 = L21_2.GetHeight(L21_2)
    L21_2.SetLocation(L21_2, (L17_2 * 2), ((L15_2 / 2) - (L22_2 / 2)), (L17_2 * 3), ((L15_2 / 2) + (L22_2 / 2)))
    L21_2.Wrap(L21_2)
    L22_2 = L21_2.GetHeight(L21_2)
    L21_2.SetLocation(L21_2, (L17_2 * 2), ((L15_2 / 2) - (L22_2 / 2)), (L17_2 * 3), ((L15_2 / 2) + (L22_2 / 2)))
    _BuildStrokes(L21_2, ((L17_2 * 2) + 2), 0, ((L17_2 * 3) - 2), L15_2)
  end
  L22_2 = MrxGuiBase.ImageWidget
  L22_2 = L22_2.new(L22_2)
  L22_2.SetColor(L22_2, 84, 79, 57)
  L22_2.SetOwner(L22_2, A0_2)
  L23_2 = L22_2.CustomData
  L26_2 = {}
  L26_2.y = 1
  L26_2.y2 = 1
  L23_2.nClosePoint = L22_2.AddAnimationPoint(L22_2, L26_2)
  L23_2 = L22_2.CustomData
  L26_2 = {}
  L26_2.y = 1
  L26_2.y2 = 1
  L23_2.nOpenPoint = L22_2.AddAnimationPoint(L22_2, L26_2)
  L23_2 = L22_2.CustomData
  L26_2 = {}
  L26_2.TranslucencyLevel = 255
  L23_2.nPulseHighPoint = L22_2.AddAnimationPoint(L22_2, L26_2)
  L23_2 = L22_2.CustomData
  L26_2 = {}
  L26_2.TranslucencyLevel = 100
  L23_2.nPulseLowPoint = L22_2.AddAnimationPoint(L22_2, L26_2)
  L22_2.SetIgnoresPause(L22_2, true)
  L22_2.SetLocation(L22_2, 0, 0, L17_2, L15_2)
  L23_2 = L14_2.CustomData
  L24_2 = {}
  L24_2[1] = L18_2
  L24_2[2] = L19_2
  L24_2[3] = L21_2
  L23_2.tOptions = L24_2
  L23_2 = L14_2.CustomData
  L23_2.oCursor = L22_2
  L14_2.SetOption = _SetScrollOption
  L14_2.AddChild(L14_2, L22_2)
  L14_2.AddChild(L14_2, L18_2)
  L14_2.AddChild(L14_2, L19_2)
  if L21_2 then
    L14_2.AddChild(L14_2, L21_2)
  end
  L23_2 = MrxGuiBase.TextWidget
  L23_2 = L23_2.new(L23_2)
  L23_2.SetLocation(L23_2, 0, 0, (L8_2 - (L9_2 * 2)), 10)
  L23_2.SetFont(L23_2, _ksFont)
  L23_2.SetScale(L23_2, _knScale)
  L23_2.SetColor(L23_2, _knTextR, _knTextG, _knTextB)
  L23_2.SetJustification(L23_2, "center")
  L23_2.SetText(L23_2, "[move] [PDA.Common.MoveSelection]  [confirm] [Generic.Confirm]")
  L23_2.SetOwner(L23_2, uPlayerGuid)
  L24_2 = L23_2.GetHeight(L23_2)
  L25_2 = MrxGuiBase.Widget
  L25_2 = L25_2.new(L25_2)
  L25_2.SetOwner(L25_2, A0_2)
  L26_2 = MrxGuiBase.ImageWidget
  L26_2 = L26_2.new(L26_2)
  L27_2 = MrxGuiBase.ImageWidget
  L27_2 = L27_2.new(L27_2)
  L28_2 = MrxGuiBase.ImageWidget
  L28_2 = L28_2.new(L28_2)
  L26_2.SetOwner(L26_2, A0_2)
  L27_2.SetOwner(L27_2, A0_2)
  L28_2.SetOwner(L28_2, A0_2)
  L29_2 = 0.6666667
  L30_2 = 447 * L29_2
  L31_2 = 48 * L29_2
  L26_2.SetLocation(L26_2, 0, 0, L30_2, L31_2)
  L26_2.SetTexture(L26_2, "global_gui_hud02")
  L26_2.SetTextureCoordinates(L26_2, 0, 0.2734375, 0.8730469, 0.1796875)
  L32_2 = ((L13_2 + L15_2) + (L10_2 * 4)) - L31_2
  L27_2.SetLocation(L27_2, 0, (48 * L29_2), L30_2, L32_2)
  L27_2.SetTexture(L27_2, "global_gui_hud02")
  L27_2.SetTextureCoordinates(L27_2, 0, 0.083984375, 0.8730469, 0.17382812)
  L28_2.SetLocation(L28_2, 0, L32_2, L30_2, (L32_2 + L31_2))
  L28_2.SetTexture(L28_2, "global_gui_hud02")
  L28_2.SetTextureCoordinates(L28_2, 0, 0.1796875, 0.8730469, 0.2734375)
  L25_2.SetLocation(L25_2, 0, 0, L30_2, (L32_2 + L31_2))
  L25_2.AddChild(L25_2, L26_2)
  L25_2.AddChild(L25_2, L27_2)
  L25_2.AddChild(L25_2, L28_2)
  L33_2 = MrxGuiBase.Widget
  L33_2 = L33_2.new(L33_2)
  L33_2.SetLocation(L33_2, 0, 0, L8_2, ((L13_2 + L15_2) + (L10_2 * 4)))
  L33_2.SetOwner(L33_2, A0_2)
  L33_2.AddChild(L33_2, L25_2)
  if L11_2 then
    L33_2.AddChild(L33_2, L11_2)
  end
  if L12_2 then
    L33_2.AddChild(L33_2, L12_2)
  end
  L33_2.AddChild(L33_2, L14_2)
  L33_2.AddChild(L33_2, L23_2)
  L34_2 = L33_2.CustomData
  L34_2.oOptions = L14_2
  L34_2 = L33_2.CustomData
  L34_2.oScroll = L12_2
  L34_2 = L33_2.CustomData
  L34_2.oCursor = L22_2
  if L12_2 then
    L12_2.SetLocation(L12_2, L9_2, L10_2)
  else
    L11_2.SetLocation(L11_2, L9_2, L10_2)
  end
  L14_2.SetLocation(L14_2, L9_2, (L13_2 + (L10_2 * 1.75)))
  L23_2.SetLocation(L23_2, L9_2, ((L13_2 + (L10_2 * 2.25)) + L15_2))
  L33_2.Close = CloseScrollingBox
  L34_2 = L33_2.CustomData
  L34_2.nScroll = 0
  L34_2 = L33_2.CustomData
  L34_2.nPadScroll = 0
  L34_2 = L33_2.CustomData
  L34_2.nAnalogScroll = 0
  L34_2 = L33_2.CustomData
  L34_2.nNumOptions = L16_2
  L34_2 = L33_2.CustomData
  L34_2.nSelected = 1
  L34_2 = L33_2.CustomData
  L34_2.fCallback = A3_2
  L34_2 = L33_2.CustomData
  L34_2.tCallbackData = A4_2
  L33_2.SetEventHandler(L33_2, "GuiUpdate", _HandleScrollUpdate)
  L33_2.SetEventHandler(L33_2, "ControllerInput", _HandleScrollInput)
  Pulse(L22_2)
  return L33_2
end

_BuildScrollingDialogBox = L3_1

function L3_1(A0_2, A1_2, A2_2, A3_2, A4_2)
  local L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2
  L5_2 = 128
  L6_2 = 128
  L7_2 = 96
  L8_2 = 255
  L9_2 = 1
  L10_2 = MrxGuiBase.ImageWidget
  L10_2 = L10_2.new(L10_2)
  L10_2.SetColor(L10_2, L5_2, L6_2, L7_2, L8_2)
  L10_2.SetLocation(L10_2, A1_2, A2_2, (A1_2 + L9_2), A4_2)
  L11_2 = MrxGuiBase.ImageWidget
  L11_2 = L11_2.new(L11_2)
  L11_2.SetColor(L11_2, L5_2, L6_2, L7_2, L8_2)
  L11_2.SetLocation(L11_2, A1_2, A2_2, A3_2, (A2_2 + L9_2))
  L12_2 = MrxGuiBase.ImageWidget
  L12_2 = L12_2.new(L12_2)
  L12_2.SetColor(L12_2, L5_2, L6_2, L7_2, L8_2)
  L12_2.SetLocation(L12_2, (A3_2 - L9_2), A2_2, A3_2, A4_2)
  L13_2 = MrxGuiBase.ImageWidget
  L13_2 = L13_2.new(L13_2)
  L13_2.SetColor(L13_2, L5_2, L6_2, L7_2, L8_2)
  L13_2.SetLocation(L13_2, A1_2, (A4_2 - L9_2), A3_2, A4_2)
  A0_2.AddChild(A0_2, L10_2)
  A0_2.AddChild(A0_2, L11_2)
  A0_2.AddChild(A0_2, L12_2)
  A0_2.AddChild(A0_2, L13_2)
end

_BuildStrokes = L3_1

function L3_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2
  L2_2 = A0_2.CustomData.oScroll
  if L2_2 then
    L2_2 = math.abs(A0_2.CustomData.nScroll)
    if 0.5 < L2_2 then
      L2_2 = (A1_2 * L2_1) * A0_2.CustomData.nScroll
      L3_2 = A0_2.CustomData.oScroll
      L3_2.OffsetText(L3_2, L2_2)
    end
  end
end

_HandleScrollUpdate = L3_1

function L3_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L2_2 = MrxGuiBase.Joystick.BUTTON_PAD1_D
  L3_2 = A1_2.ButtonPress
  if L2_2 == L3_2 then
    L2_2 = A0_2.CustomData
    L2_2.nPadScroll = -1
  else
    L2_2 = MrxGuiBase.Joystick.BUTTON_PAD1_U
    L3_2 = A1_2.ButtonPress
    if L2_2 == L3_2 then
      L2_2 = A0_2.CustomData
      L2_2.nPadScroll = 1
    else
      L2_2 = A0_2.CustomData
      L2_2.nPadScroll = 0
    end
  end
  L2_2 = A0_2.CustomData.nAnalogScroll
  if -1 == L2_2 then
    L2_2 = MrxGuiBase.Joystick.BUTTON_L_STICK_D
    L3_2 = A1_2.ButtonReleased
    if L2_2 == L3_2 then
      L2_2 = A0_2.CustomData
      L2_2.nAnalogScroll = 0
    end
  else
    L2_2 = MrxGuiBase.Joystick.BUTTON_L_STICK_D
    L3_2 = A1_2.ButtonPress
    if L2_2 == L3_2 then
      L2_2 = A0_2.CustomData
      L2_2.nAnalogScroll = -1
    end
  end
  L2_2 = A0_2.CustomData.nAnalogScroll
  if 1 == L2_2 then
    L2_2 = MrxGuiBase.Joystick.BUTTON_L_STICK_U
    L3_2 = A1_2.ButtonReleased
    if L2_2 == L3_2 then
      L2_2 = A0_2.CustomData
      L2_2.nAnalogScroll = 0
    end
  else
    L2_2 = MrxGuiBase.Joystick.BUTTON_L_STICK_U
    L3_2 = A1_2.ButtonPress
    if L2_2 == L3_2 then
      L2_2 = A0_2.CustomData
      L2_2.nAnalogScroll = 1
    end
  end
  L2_2 = A0_2.CustomData
  L2_2.nScroll = (A0_2.CustomData.nPadScroll + A0_2.CustomData.nAnalogScroll)
  L2_2 = 0
  L3_2 = MrxGuiBase.Joystick.BUTTON_PAD1_L
  L4_2 = A1_2.ButtonPress
  if L3_2 ~= L4_2 then
    L3_2 = MrxGuiBase.Joystick.BUTTON_L_STICK_L
    L4_2 = A1_2.ButtonPress
    if L3_2 ~= L4_2 then
      goto lbl_85
    end
  end
  L2_2 = -1
  goto lbl_98
  ::lbl_85::
  L3_2 = MrxGuiBase.Joystick.BUTTON_PAD1_R
  L4_2 = A1_2.ButtonPress
  if L3_2 ~= L4_2 then
    L3_2 = MrxGuiBase.Joystick.BUTTON_L_STICK_R
    L4_2 = A1_2.ButtonPress
    if L3_2 ~= L4_2 then
      goto lbl_98
    end
  end
  L2_2 = 1
  ::lbl_98::
  L3_2 = math.abs(L2_2)
  if 0.5 < L3_2 then
    L3_2 = A0_2.CustomData
    L3_2.nSelected = (A0_2.CustomData.nSelected + L2_2)
    while true do
      L3_2 = A0_2.CustomData.nSelected
      L4_2 = A0_2.CustomData.nNumOptions
      if not (L3_2 > L4_2) then
        break
      end
      L3_2 = A0_2.CustomData
      L3_2.nSelected = (A0_2.CustomData.nSelected - A0_2.CustomData.nNumOptions)
    end
    while true do
      L3_2 = A0_2.CustomData.nSelected
      if not (L3_2 < 1) then
        break
      end
      L3_2 = A0_2.CustomData
      L3_2.nSelected = (A0_2.CustomData.nSelected + A0_2.CustomData.nNumOptions)
    end
    Sound.CueSound(0, _ksChangeSound)
    L3_2 = A0_2.CustomData.oOptions
    L3_2.SetOption(L3_2, A0_2.CustomData.nSelected)
  end
  L3_2 = MrxGuiBase.Joystick.BUTTON_PAD2_D
  L4_2 = A1_2.ButtonPress
  if L3_2 == L4_2 then
    A0_2.Close(A0_2)
    Sound.CueSound(0, _ksAcceptSound)
    _CallScrollBoxCallback(A0_2.CustomData.fCallback, A0_2.CustomData.tCallbackData, A0_2.CustomData.nSelected)
  else
    L3_2 = MrxGuiBase.Joystick.BUTTON_PAD2_R
    L4_2 = A1_2.ButtonPress
    if L3_2 == L4_2 then
      A0_2.Close(A0_2)
      Sound.CueSound(0, _ksCancelSound)
      _CallScrollBoxCallback(A0_2.CustomData.fCallback, A0_2.CustomData.tCallbackData, 2)
    end
  end
end

_HandleScrollInput = L3_1

function L3_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2
  if A0_2 then
    if not A1_2 then
      A1_2 = {}
    end
    table.insert(A1_2, A2_2)
    L4_2 = unpack
    L5_2 = A1_2
    L4_2, L5_2 = L4_2(L5_2)
    A0_2(L4_2, L5_2)
  end
end

_CallScrollBoxCallback = L3_1

function L3_1(A0_2)
  local L1_2, L2_2
  MrxGuiBase.ReleaseControlFocus(A0_2)
  MrxGuiBase.RemoveWidgetWithChildren(A0_2)
  A0_2.DeleteWithChildren(A0_2)
end

CloseScrollingBox = L3_1

function L3_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2
  L2_2 = A0_2.CustomData.tOptions[A1_2]
  if not L2_2 then
    return
  end
  L2_2 = A0_2.CustomData.tOptions[A1_2]
  L3_2 = L2_2
  L2_2 = L2_2.GetLocation
  L2_2, L3_2, L4_2 = L2_2(L3_2)
  L6_2 = A0_2
  L5_2 = A0_2.GetLocation
  L5_2, L6_2, L7_2, L8_2 = L5_2(L6_2)
  L9_2 = A0_2.CustomData
  L9_2.nSelectedIndex = A1_2
  L9_2 = A0_2.CustomData.oCursor
  L10_2 = (L6_2 + L8_2) * 0.5
  L14_2 = {}
  L14_2.y = L10_2
  L14_2.y2 = L10_2
  L9_2.SetAnimationPoint(L9_2, L9_2.CustomData.nClosePoint, L14_2)
  L14_2 = {}
  L14_2.y = L6_2
  L14_2.y2 = L8_2
  L9_2.SetAnimationPoint(L9_2, L9_2.CustomData.nOpenPoint, L14_2)
  L9_2.SetLocation(L9_2, nil, L6_2, nil, L8_2)
  L17_2 = {}
  L17_2[1] = L2_2
  L17_2[2] = L4_2
  L9_2.AnimateToPoint(L9_2, L9_2.CustomData.nClosePoint, 0.075, true, _CompleteScrollAnimation, L17_2)
end

_SetScrollOption = L3_1

function L3_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  A0_2.SetLocation(A0_2, A1_2, nil, A2_2, nil)
  A0_2.AnimateToPoint(A0_2, A0_2.CustomData.nOpenPoint, 0.075, true, Pulse)
end

_CompleteScrollAnimation = L3_1

function L3_1(A0_2, A1_2, A2_2, A3_2, A4_2, A5_2)
  local L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2, L23_2
  L6_2 = MrxGuiBase.Widget
  L6_2 = L6_2.new(L6_2)
  L6_2.SetLocation(L6_2, 0, 0, A4_2, A5_2)
  L7_2 = A1_2.GetHeight(A1_2)
  L8_2 = A1_2.SplitIntoLines(A1_2)
  L10_2 = MrxGuiBase.Widget
  L10_2 = L10_2.new(L10_2)
  L14_2 = 0
  L10_2.SetLocation(L10_2, 0, L14_2, A4_2, L7_2)
  L11_2 = L10_2.CustomData
  L11_2.nLineHeight = (L7_2 / #L8_2)
  L11_2 = 0
  L12_2 = pairs
  L13_2 = L8_2
  L12_2, L13_2, L14_2 = L12_2(L13_2)
  for L15_2, L16_2 in L12_2, L13_2, L14_2 do
    L16_2.SetLocation(L16_2, 0, L11_2)
    L16_2.SetOwner(L16_2, A0_2)
    L10_2.AddChild(L10_2, L16_2)
    L11_2 = L11_2 + L9_2
  end
  L12_2 = L10_2.CustomData
  L12_2.tLines = L8_2
  L12_2 = L10_2.CustomData
  L12_2.nMaxHeight = A5_2
  L10_2.UpdateAlpha = _UpdateTextAlpha
  L10_2.ParentWidget = L6_2
  L10_2.SetOwner(L10_2, A0_2)
  L10_2.UpdateAlpha(L10_2)
  L12_2 = MrxGuiBase.ImageWidget
  L12_2 = L12_2.new(L12_2)
  L13_2 = MrxGuiBase.ImageWidget
  L13_2 = L13_2.new(L13_2)
  L14_2 = (A5_2 - (L0_1 * 2)) - 2
  L15_2 = L14_2 * (A5_2 / L7_2)
  L12_2.SetColor(L12_2, 64, 64, 48)
  L13_2.SetColor(L13_2, 128, 128, 96)
  L12_2.SetLocation(L12_2, (A2_2 + A4_2), A3_2, ((A2_2 + A4_2) + L0_1), (A3_2 + A5_2))
  L13_2.SetLocation(L13_2, A4_2, (L0_1 + 1), (A4_2 + L0_1), ((L15_2 + L0_1) + 1))
  L16_2 = MrxGuiBase.ImageWidget
  L16_2 = L16_2.new(L16_2)
  L16_2.SetTexture(L16_2, "global_gui_hud02")
  L16_2.SetTextureCoordinates(L16_2, 0.001953, 0.947266, 0.162109, 0.986328)
  L16_2.SetLocation(L16_2, (0 + A4_2), 0, ((0 + A4_2) + L0_1), L0_1)
  L17_2 = MrxGuiBase.ImageWidget
  L17_2 = L17_2.new(L17_2)
  L17_2.SetTexture(L17_2, "global_gui_hud02")
  L17_2.SetTextureCoordinates(L17_2, 0.001953, 0.986328, 0.162109, 0.947266)
  L17_2.SetLocation(L17_2, A4_2, (A5_2 - L0_1), (A4_2 + L0_1), A5_2)
  L18_2 = L13_2.CustomData
  L18_2.nHeight = L15_2
  L18_2 = L13_2.CustomData
  L18_2.nTotalHeight = L14_2
  L6_2.AddChild(L6_2, L12_2)
  L6_2.AddChild(L6_2, L13_2)
  L6_2.AddChild(L6_2, L16_2)
  L6_2.AddChild(L6_2, L17_2)
  L6_2.AddChild(L6_2, L10_2)
  L18_2 = L6_2.CustomData
  L18_2.oBarBg = L12_2
  L18_2 = L6_2.CustomData
  L18_2.oBar = L13_2
  L18_2 = L6_2.CustomData
  L18_2.oTextContainer = L10_2
  L18_2 = L6_2.CustomData
  L18_2.nHeight = A5_2
  L18_2 = L6_2.CustomData
  L18_2.nTextHeight = L7_2
  L18_2 = L6_2.CustomData
  L18_2.nOffset = 0
  L6_2.OffsetText = _OffsetText
  L6_2.SetLocation(L6_2, A2_2, A3_2)
  return L6_2
end

_CreateScrollableWindow = L3_1

function L3_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  L3_2 = A0_2
  L2_2 = A0_2.GetLocation
  L2_2, L3_2 = L2_2(L3_2)
  L4_2 = A0_2.CustomData
  L4_2.nOffset = (A0_2.CustomData.nOffset + A1_2)
  L4_2 = A0_2.CustomData
  L4_2.nOffset = _Clamp(A0_2.CustomData.nOffset, (A0_2.CustomData.nHeight - A0_2.CustomData.nTextHeight), 0)
  L4_2 = A0_2.CustomData.oTextContainer
  L4_2.SetLocation(L4_2, L2_2, (L3_2 + A0_2.CustomData.nOffset))
  L4_2 = A0_2.CustomData.oTextContainer
  L4_2.UpdateAlpha(L4_2)
  L4_2 = A0_2.CustomData.oBarBg
  L5_2 = L4_2
  L4_2 = L4_2.GetLocation
  L4_2, L5_2 = L4_2(L5_2)
  L6_2 = A0_2.CustomData.oBar
  L6_2.SetLocation(L6_2, L4_2, (((L5_2 + L0_1) + 1) - ((A0_2.CustomData.nOffset / A0_2.CustomData.nTextHeight) * A0_2.CustomData.oBar.CustomData.nTotalHeight)))
end

_OffsetText = L3_1

function L3_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2
  L3_2 = math.max
  L4_2 = math.min(A0_2, A2_2)
  L5_2 = A1_2
  return L3_2(L4_2, L5_2)
end

_Clamp = L3_1

function L3_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2
  L1_2 = A0_2.CustomData.nLineHeight * 0.5
  L2_2 = A0_2.ParentWidget
  L3_2 = L2_2
  L2_2 = L2_2.GetLocation
  L2_2, L3_2 = L2_2(L3_2)
  L4_2 = L3_2 - (L1_2 * 0.5)
  L5_2 = ((L3_2 + A0_2.CustomData.nMaxHeight) - L1_2) - (L1_2 * 0.5)
  L6_2 = pairs
  L7_2 = A0_2.CustomData.tLines
  L6_2, L7_2, L8_2 = L6_2(L7_2)
  for L9_2, L10_2 in L6_2, L7_2, L8_2 do
    L12_2 = L10_2
    L11_2 = L10_2.GetLocation
    L11_2, L12_2 = L11_2(L12_2)
    L3_2 = L12_2
    L2_2 = L11_2
    if L4_2 > L3_2 then
      L13_2 = math.max
      L14_2 = (1 - ((L4_2 - L3_2) / L1_2)) * 255
      L15_2 = 0
      L13_2, L14_2, L15_2 = L13_2(L14_2, L15_2)
      L10_2.SetTranslucency(L10_2, L13_2, L14_2, L15_2)
    elseif L5_2 < L3_2 then
      L13_2 = math.max
      L14_2 = (1 - ((L3_2 - L5_2) / L1_2)) * 255
      L15_2 = 0
      L13_2, L14_2, L15_2 = L13_2(L14_2, L15_2)
      L10_2.SetTranslucency(L10_2, L13_2, L14_2, L15_2)
    else
      L10_2.SetTranslucency(L10_2, 255)
    end
  end
end

_UpdateTextAlpha = L3_1

function L3_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2
  L3_2 = type(A0_2)
  if L3_2 == A1_2 then
    return A0_2
  else
    return A2_2
  end
end

_ValidateParameter = L3_1
oSystemDialogBoxFlash = nil

function L3_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2
  L3_2 = oSystemDialogBoxFlash
  if L3_2 == nil then
    L5_2 = Player.GetLocalPlayer
    L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2 = L5_2()
    L3_2 = MrxGuiBase.GetWidgetByNameAndOwner("PDA", L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2)
    if L3_2 then
      L3_2.Close(L3_2)
    end
    L3_2 = MrxGuiBase.GetWidgetByName("Pause Layout")
    if L3_2 then
      L3_2.Close(L3_2)
      Sys.RequestGameState("ingame")
    end
    L4_2 = MrxGuiBase.FlashWidget
    oSystemDialogBoxFlash = L4_2.new(L4_2)
    L4_2 = oSystemDialogBoxFlash
    L6_2 = Player.GetLocalPlayer
    L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2 = L6_2()
    L4_2.SetOwner(L4_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2)
    L4_2 = oSystemDialogBoxFlash
    L4_2.SetLocation(L4_2, 160, 120, 480, 360)
    MrxGuiBase.AddWidget(oSystemDialogBoxFlash)
    L4_2 = oSystemDialogBoxFlash
    L8_2 = {}
    L8_2[1] = oSystemDialogBoxFlash
    L8_2[2] = A0_2
    L8_2[3] = A1_2
    L8_2[4] = A2_2
    L4_2.SetSwfFile(L4_2, "dialog_box", SystemDialogBoxLoadedCallBack, L8_2)
  end
end

OpenSystemDialogBox = L3_1

function L3_1(A0_2, A1_2, A2_2, A3_2)
  local L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  if A0_2 ~= nil then
    MrxGuiBase.GetControlFocus(A0_2, true)
    L7_2 = {}
    L7_2[1] = A1_2
    L7_2[2] = A2_2
    L7_2[3] = 1
    L7_2[4] = A3_2
    A0_2.CallActionScriptCallback(A0_2, "onlineMessage", L7_2)
    A0_2.SetFlashEventHandler(A0_2, "onlineMessageClose", CloseSystemDialogBox, {})
  end
end

SystemDialogBoxLoadedCallBack = L3_1

function L3_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2
  L2_2 = {}
  L2_2[1] = 0.01
  L2_2[2] = true
  L4_2 = {}
  L4_2[1] = oSystemDialogBoxFlash
  Event.Create(Event.TimerRelative, L2_2, CloseSystemDialogBoxDelayed, L4_2)
  oSystemDialogBoxFlash = nil
end

CloseSystemDialogBox = L3_1

function L3_1(A0_2)
  local L1_2, L2_2, L3_2
  if A0_2 ~= nil then
    MrxGuiBase.ReleaseControlFocus(A0_2)
    MrxGuiBase.RemoveWidget(A0_2)
    A0_2.SetSwfFile(A0_2, nil)
    A0_2.delete(A0_2)
  end
end

CloseSystemDialogBoxDelayed = L3_1
