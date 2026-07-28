import("MrxGui")
_ksPressSound = "ui_HUD_Minigame_Press_Button"
_ksErrorSound = "ui_HUD_Minigame_Error"
_ksMashSound = "ui_HUD_Minigame_Tap_Button_lp"
_ksRecoverSound = "ui_HUD_Minigame_Tap_Button_Recover_lp"
Joystick = MrxGui.Joystick
Joystick = {
  BUTTON_PAD1_U = 1,
  BUTTON_PAD1_D = 2,
  BUTTON_PAD1_L = 3,
  BUTTON_PAD1_R = 4,
  BUTTON_PAD2_U = 5,
  BUTTON_PAD2_D = 6,
  BUTTON_PAD2_L = 7,
  BUTTON_PAD2_R = 8,
  BUTTON_L_STICK_L = 9,
  BUTTON_L_STICK_R = 10,
  BUTTON_L_STICK_U = 11,
  BUTTON_L_STICK_D = 12,
  BUTTON_R_STICK_L = 13,
  BUTTON_R_STICK_R = 14,
  BUTTON_R_STICK_U = 15,
  BUTTON_R_STICK_D = 16,
  BUTTON_ALT1_1 = 17,
  BUTTON_ALT1_2 = 18,
  BUTTON_ALT1_3 = 19,
  BUTTON_ALT2_1 = 20,
  BUTTON_ALT2_2 = 21,
  BUTTON_ALT2_3 = 22,
  BUTTON_SYS1 = 23,
  BUTTON_SYS2 = 24,
  BUTTON_L_STICK_LR = 25,
  BUTTON_USE_MELEE = 26,
  BUTTON_USE_RELOAD = 27
}

function ShowButton(uGuid, nButton, nTime, nRepeatTime, nXPosition, nYPosition, nTranslucency, bShowSparks, nElapsedTime, bFillTimer, bClockwise, bIsRecovery, bShowTimer, nScale)
  if "userdata" ~= type(uGuid) then
    return
  end
  if not (_ControllerSpriteTextureMapping[nButton] and _ControllerSpriteData[nButton] and _ControllerXboxSpriteTextureMapping[nButton]) or not _ControllerXboxSpriteData[nButton] then
    Debug.Printf("No data for given action hijack buttons")
    return
  end
  nRepeatTime = nRepeatTime or -1
  local oActionHijackDisplay = MrxGui.GetWidgetByNameAndOwner("Action Hijack", uGuid)
  if not oActionHijackDisplay then
    return
  end
  if "number" ~= type(nScale) then
    nScale = 1
  end
  local nDisplayX1, nDisplayY1, nDisplayX2, nDisplayY2 = oActionHijackDisplay:GetLocation()
  local nDisplayWOffset = oActionHijackDisplay.CustomData.nOriginalWidth * nScale * 0.5
  local nDisplayHOffset = oActionHijackDisplay.CustomData.nOriginalHeight * nScale * 0.5
  nXPosition = ((nXPosition or 0) + 1) * 320
  nYPosition = ((nYPosition or 0) + 1) * 190
  oActionHijackDisplay:SetLocation(nXPosition - nDisplayWOffset, nYPosition - nDisplayHOffset, nXPosition + nDisplayWOffset, nYPosition + nDisplayHOffset)
  oActionHijackDisplay:SetTranslucency(nTranslucency or 190)
  local oButton = oActionHijackDisplay.CustomData.oButton
  local oTimer = oActionHijackDisplay.CustomData.oTimer
  local oFail = oActionHijackDisplay.CustomData.oFail
  if Gui.IsXboxController and Gui.IsXboxController() then
    Debug.Printf("IsXboxController returned TRUE")
    oButton:SetTexture(_ControllerXboxSpriteTextureMapping[nButton])
    oButton:SetTextureSize(_ControllerXboxSpriteData[nButton][5], _ControllerXboxSpriteData[nButton][6])
    local nX1, nY1, nX2, nY2 = oActionHijackDisplay:GetLocation()
    local nMidX = nXPosition
    local nMidY = nYPosition
    local nTimerSize = 64 * nScale
    oFail:SetLocation(nX1, nY1, nX2, nY2)
    oTimer:SetLocation(nMidX - nTimerSize, nMidY - nTimerSize, nMidX + nTimerSize, nMidY + nTimerSize)
    oButton:SetLocation(nMidX - _ControllerXboxSpriteData[nButton][1] / 2 * nScale, nY1 + _ControllerXboxSpriteData[nButton][4] * nScale, nMidX + _ControllerXboxSpriteData[nButton][1] / 2 * nScale, nY1 + (_ControllerXboxSpriteData[nButton][4] + _ControllerXboxSpriteData[nButton][2]) * nScale)
    oButton:SetFrameSize(_ControllerXboxSpriteData[nButton][1], _ControllerXboxSpriteData[nButton][2])
  else
    Debug.Printf("IsXboxController returned FALSE")
    oButton:SetTexture(_ControllerSpriteTextureMapping[nButton])
    oButton:SetTextureSize(_ControllerSpriteData[nButton][5], _ControllerSpriteData[nButton][6])
    local nX1, nY1, nX2, nY2 = oActionHijackDisplay:GetLocation()
    local nMidX = nXPosition
    local nMidY = nYPosition
    local nTimerSize = 64 * nScale
    oFail:SetLocation(nX1, nY1, nX2, nY2)
    oTimer:SetLocation(nMidX - nTimerSize, nMidY - nTimerSize, nMidX + nTimerSize, nMidY + nTimerSize)
    oButton:SetLocation(nMidX - _ControllerSpriteData[nButton][1] / 2 * nScale, nY1 + _ControllerSpriteData[nButton][4] * nScale, nMidX + _ControllerSpriteData[nButton][1] / 2 * nScale, nY1 + (_ControllerSpriteData[nButton][4] + _ControllerSpriteData[nButton][2]) * nScale)
    oButton:SetFrameSize(_ControllerSpriteData[nButton][1], _ControllerSpriteData[nButton][2])
  end
  if bShowSparks then
    local oSparkL = oActionHijackDisplay.CustomData.oSparkL
    local oSparkR = oActionHijackDisplay.CustomData.oSparkR
    oSparkL:SetVisible(true)
    oSparkR:SetVisible(true)
    oSparkL:PlayAnimation(0, 2, 0.25, true)
    oSparkR:PlayAnimation(0, 2, 0.25, true)
  else
    local oSparkL = oActionHijackDisplay.CustomData.oSparkL
    local oSparkR = oActionHijackDisplay.CustomData.oSparkR
    oSparkL:SetVisible(false)
    oSparkR:SetVisible(false)
  end
  if nil == bFillTimer then
    bFillTimer = false
  end
  if nil == bClockwise then
    bClockwise = true
  end
  local nLastFrame = 1
  if nButton >= Joystick.BUTTON_L_STICK_L and nButton <= Joystick.BUTTON_R_STICK_D then
    nLastFrame = 2
  end
  oButton:SetFrame(0)
  if 0 < nRepeatTime then
    oButton:PlayAnimation(0, nLastFrame, nRepeatTime, true)
  else
    oButton:SetFrame(nLastFrame)
    oButton:HaltAnimation()
  end
  oButton:SetVisible(true)
  oActionHijackDisplay.CustomData.oFail:SetVisible(false)
  if nil == bShowTimer then
    bShowTimer = true
  end
  if bShowTimer then
    oTimer:SetTexture("countdown_circle")
    oTimer:SetClockAnimation(nElapsedTime or 0, nTime, bFillTimer, bClockwise)
  else
    oTimer:SetTexture("icon_hijack_glow")
    oTimer:SetClockAnimation(1, 1, true, bClockwise)
  end
  oTimer:SetVisible(true)
  if bIsRecovery then
    if not oActionHijackDisplay.CustomData.bRecoverSoundLooping then
      if oActionHijackDisplay.CustomData.bSoundLooping then
        oActionHijackDisplay.CustomData.bSoundLooping = false
        Sound.StopSound(0, _ksMashSound)
      end
      Sound.CueSound(0, _ksRecoverSound)
      oActionHijackDisplay.CustomData.bRecoverSoundLooping = true
    end
  elseif bShowSparks and not oActionHijackDisplay.CustomData.bSoundLooping then
    if oActionHijackDisplay.CustomData.bRecoverSoundLooping then
      oActionHijackDisplay.CustomData.bRecoverSoundLooping = false
      Sound.StopSound(0, _ksRecoverSound)
    end
    Sound.CueSound(0, _ksMashSound)
    oActionHijackDisplay.CustomData.bSoundLooping = true
  else
    Sound.CueSound(0, _ksPressSound)
  end
end

function HideButton(uGuid)
  if "userdata" ~= type(uGuid) then
    return
  end
  local oActionHijackDisplay = MrxGui.GetWidgetByNameAndOwner("Action Hijack", uGuid)
  if oActionHijackDisplay then
    oActionHijackDisplay:SetVisible(false)
    oActionHijackDisplay.CustomData.oButton:HaltAnimation()
    oActionHijackDisplay.CustomData.oSparkL:HaltAnimation()
    oActionHijackDisplay.CustomData.oSparkR:HaltAnimation()
    if oActionHijackDisplay.CustomData.bSoundLooping then
      oActionHijackDisplay.CustomData.bSoundLooping = false
      Sound.StopSound(0, _ksMashSound)
    end
    if oActionHijackDisplay.CustomData.bRecoverSoundLooping then
      oActionHijackDisplay.CustomData.bRecoverSoundLooping = false
      Sound.StopSound(0, _ksRecoverSound)
    end
  end
end

function ShowFail(uGuid, nDuration)
  if "userdata" ~= type(uGuid) then
    return
  end
  local oActionHijackDisplay = MrxGui.GetWidgetByNameAndOwner("Action Hijack", uGuid)
  if oActionHijackDisplay then
    oActionHijackDisplay:SetVisible(false)
    oActionHijackDisplay.CustomData.oButton:HaltAnimation()
    oActionHijackDisplay.CustomData.oSparkL:HaltAnimation()
    oActionHijackDisplay.CustomData.oSparkR:HaltAnimation()
    nDuration = nDuration or 0.5
    local oFail = oActionHijackDisplay.CustomData.oFail
    oFail:SetVisible(true)
    oFail:AnimateToPoint(oFail.CustomData.nStartPoint, 0, true)
    oFail:AnimateToPoint(oFail.CustomData.nEndPoint, nDuration, false, oFail.SetVisible, {false})
    Sound.CueSound(0, _ksErrorSound)
    if oActionHijackDisplay.CustomData.bSoundLooping then
      Sound.StopSound(0, _ksMashSound)
      oActionHijackDisplay.CustomData.bSoundLooping = false
    end
    if oActionHijackDisplay.CustomData.bRecoverSoundLooping then
      oActionHijackDisplay.CustomData.bRecoverSoundLooping = false
      Sound.StopSound(0, _ksRecoverSound)
    end
  end
end

function GetElapsedTime(uGuid)
  if "userdata" ~= type(uGuid) then
    return nil
  end
  local oActionHijackDisplay = MrxGui.GetWidgetByNameAndOwner("Action Hijack", uGuid)
  if oActionHijackDisplay then
    return oActionHijackDisplay.CustomData.oTimer:GetClockElapsedTime()
  end
  return nil
end

function SetDisplayVisible(uGuid, bVisible)
  if "userdata" ~= type(uGuid) then
    return
  end
  local oActionHijackDisplay = MrxGui.GetWidgetByNameAndOwner("Action Hijack", uGuid)
  if oActionHijackDisplay then
    oActionHijackDisplay:SetVisible(bVisible)
    oActionHijackDisplay.CustomData.oButton:HaltAnimation()
    oActionHijackDisplay.CustomData.oSparkL:HaltAnimation()
    oActionHijackDisplay.CustomData.oSparkR:HaltAnimation()
    oActionHijackDisplay.CustomData.oTimer:SetVisible(false)
    oActionHijackDisplay.CustomData.oButton:SetVisible(false)
  end
end

function SetDisplayButton()
  Debug.Printf("Deprecated.")
end

function SetDisplayMashAnimation()
  Debug.Printf("Deprecated.")
end

function _HandleInitialization(oWidget)
  local nX1, nY1, nX2, nY2 = oWidget:GetLocation()
  local nMidX = (nX1 + nX2) / 2
  local nMidY = (nY1 + nY2) / 2
  oWidget.CustomData.nOriginalWidth = nX2 - nX1
  oWidget.CustomData.nOriginalHeight = nY2 - nY1
  local oTimer = MrxGui.ImageWidget:new()
  oTimer:SetTexture("countdown_circle")
  oTimer:SetLocation(nMidX - 64, nMidY - 64, nMidX + 64, nMidY + 64)
  oTimer:SetVisible(false)
  oTimer:SetOwner(oWidget:GetOwner())
  oTimer:SetAnchoring("center", "center")
  MrxGui.AddWidget(oTimer)
  oWidget:AddChild(oTimer)
  oWidget.CustomData.oTimer = oTimer
  local oButton = MrxGui.SpriteWidget:new()
  oButton:SetTextureSize(128, 128)
  oButton:SetFrameSize(128, 64)
  oButton:SetLocation(nMidX - 49.5, nMidY - 42.5, nMidX + 49.5, nMidY + 42.5)
  oButton:SetVisible(false)
  oButton:SetOwner(oWidget:GetOwner())
  oButton:SetAnchoring("center", "center")
  MrxGui.AddWidget(oButton)
  oWidget:AddChild(oButton)
  oWidget.CustomData.oButton = oButton
  local oFail = MrxGui.ImageWidget:new()
  oFail:SetLocation(nMidX - 64, nMidY - 64, nMidX + 64, nMidY + 64)
  oFail:SetTexture("icon_fail")
  oFail:SetOwner(oWidget:GetOwner())
  oFail:SetVisible(false)
  oFail.CustomData.nStartPoint = oFail:AddAnimationPoint({TranslucencyLevel = 255})
  oFail.CustomData.nEndPoint = oFail:AddAnimationPoint({TranslucencyLevel = 0})
  oFail:SetAnchoring("center", "center")
  MrxGui.AddWidget(oFail)
  oWidget:AddChild(oFail)
  oWidget.CustomData.oFail = oFail
  local oSparkL = MrxGui.SpriteWidget:new()
  oSparkL:SetTexture("icon_sparks_left")
  oSparkL:SetTextureSize(128, 128)
  oSparkL:SetFrameSize(64, 64)
  oSparkL:SetVisible(false)
  oSparkL:SetLocation(nMidX - 64, nMidY - 24, nMidX - 16, nMidY + 24)
  oSparkL:SetOwner(oWidget:GetOwner())
  oSparkL:SetAnchoring("center", "center")
  MrxGui.AddWidget(oSparkL)
  oWidget:AddChild(oSparkL)
  oWidget.CustomData.oSparkL = oSparkL
  local oSparkR = MrxGui.SpriteWidget:new()
  oSparkR:SetTexture("icon_sparks_right")
  oSparkR:SetTextureSize(128, 128)
  oSparkR:SetFrameSize(64, 64)
  oSparkR:SetVisible(false)
  oSparkR:SetLocation(nMidX + 16, nMidY - 24, nMidX + 64, nMidY + 24)
  oSparkR:SetOwner(oWidget:GetOwner())
  oSparkR:SetAnchoring("center", "center")
  MrxGui.AddWidget(oSparkR)
  oWidget:AddChild(oSparkR)
  oWidget.CustomData.oSparkR = oSparkR
  Gui.LoadTexture("countdown_circle", "texture")
  Gui.LoadTexture("icon_hijack_button_A", "texture")
  Gui.LoadTexture("icon_hijack_button_B", "texture")
  Gui.LoadTexture("icon_hijack_button_X", "texture")
  Gui.LoadTexture("icon_hijack_button_Y", "texture")
  Gui.LoadTexture("icon_hijack_joystick_down", "texture")
  Gui.LoadTexture("icon_hijack_joystick_up", "texture")
  Gui.LoadTexture("icon_hijack_joystick_left", "texture")
  Gui.LoadTexture("icon_hijack_joystick_right", "texture")
  Gui.LoadTexture("icon_fail", "texture")
  Gui.LoadTexture("icon_sparks_left", "texture")
  Gui.LoadTexture("icon_sparks_right", "texture")
  Gui.LoadTexture("icon_hijack_joystick_leftright", "texture")
  Gui.LoadTexture("Use_Melee", "texture")
  Gui.LoadTexture("Use_Reload", "texture")
  Gui.LoadTexture("icon_hijack_xbox_button_A", "texture")
  Gui.LoadTexture("icon_hijack_xbox_button_B", "texture")
  Gui.LoadTexture("icon_hijack_xbox_button_X", "texture")
  Gui.LoadTexture("icon_hijack_xbox_button_Y", "texture")
  Gui.LoadTexture("icon_hijack_xbox_joystick_down", "texture")
  Gui.LoadTexture("icon_hijack_xbox_joystick_up", "texture")
  Gui.LoadTexture("icon_hijack_xbox_joystick_left", "texture")
  Gui.LoadTexture("icon_hijack_xbox_joystick_right", "texture")
  Gui.LoadTexture("icon_hijack_xbox_joystick_leftright", "texture")
  Gui.LoadTexture("xbox_Use_Melee", "texture")
  Gui.LoadTexture("xbox_Use_Reload", "texture")
end

_ControllerSpriteTextureMapping = {}
_ControllerSpriteTextureMapping[Joystick.BUTTON_PAD2_U] = "icon_hijack_button_Y"
_ControllerSpriteTextureMapping[Joystick.BUTTON_PAD2_D] = "icon_hijack_button_A"
_ControllerSpriteTextureMapping[Joystick.BUTTON_PAD2_L] = "icon_hijack_button_X"
_ControllerSpriteTextureMapping[Joystick.BUTTON_PAD2_R] = "icon_hijack_button_B"
_ControllerSpriteTextureMapping[Joystick.BUTTON_L_STICK_L] = "icon_hijack_joystick_left"
_ControllerSpriteTextureMapping[Joystick.BUTTON_L_STICK_R] = "icon_hijack_joystick_right"
_ControllerSpriteTextureMapping[Joystick.BUTTON_L_STICK_U] = "icon_hijack_joystick_up"
_ControllerSpriteTextureMapping[Joystick.BUTTON_L_STICK_D] = "icon_hijack_joystick_down"
_ControllerSpriteTextureMapping[Joystick.BUTTON_L_STICK_LR] = "icon_hijack_joystick_leftright"
_ControllerSpriteTextureMapping[Joystick.BUTTON_USE_MELEE] = "Use_Melee"
_ControllerSpriteTextureMapping[Joystick.BUTTON_USE_RELOAD] = "Use_Reload"
_ControllerXboxSpriteTextureMapping = {}
_ControllerXboxSpriteTextureMapping[Joystick.BUTTON_PAD2_U] = "icon_hijack_xbox_button_Y"
_ControllerXboxSpriteTextureMapping[Joystick.BUTTON_PAD2_D] = "icon_hijack_xbox_button_A"
_ControllerXboxSpriteTextureMapping[Joystick.BUTTON_PAD2_L] = "icon_hijack_xbox_button_X"
_ControllerXboxSpriteTextureMapping[Joystick.BUTTON_PAD2_R] = "icon_hijack_xbox_button_B"
_ControllerXboxSpriteTextureMapping[Joystick.BUTTON_L_STICK_L] = "icon_hijack_xbox_joystick_left"
_ControllerXboxSpriteTextureMapping[Joystick.BUTTON_L_STICK_R] = "icon_hijack_xbox_joystick_right"
_ControllerXboxSpriteTextureMapping[Joystick.BUTTON_L_STICK_U] = "icon_hijack_xbox_joystick_up"
_ControllerXboxSpriteTextureMapping[Joystick.BUTTON_L_STICK_D] = "icon_hijack_xbox_joystick_down"
_ControllerXboxSpriteTextureMapping[Joystick.BUTTON_L_STICK_LR] = "icon_hijack_xbox_joystick_leftright"
_ControllerXboxSpriteTextureMapping[Joystick.BUTTON_USE_MELEE] = "xbox_Use_Melee"
_ControllerXboxSpriteTextureMapping[Joystick.BUTTON_USE_RELOAD] = "xbox_Use_Reload"
_ControllerSpriteData = {}
_ControllerSpriteData[Joystick.BUTTON_PAD2_U] = {
  64,
  128,
  0,
  0,
  128,
  128
}
_ControllerSpriteData[Joystick.BUTTON_PAD2_L] = _ControllerSpriteData[Joystick.BUTTON_PAD2_U]
_ControllerSpriteData[Joystick.BUTTON_PAD2_R] = _ControllerSpriteData[Joystick.BUTTON_PAD2_U]
_ControllerSpriteData[Joystick.BUTTON_PAD2_D] = _ControllerSpriteData[Joystick.BUTTON_PAD2_U]
_ControllerSpriteData[Joystick.BUTTON_L_STICK_L] = {
  116,
  128,
  0,
  0,
  512,
  128
}
_ControllerSpriteData[Joystick.BUTTON_L_STICK_R] = _ControllerSpriteData[Joystick.BUTTON_L_STICK_L]
_ControllerSpriteData[Joystick.BUTTON_L_STICK_U] = {
  116,
  128,
  0,
  0,
  512,
  128
}
_ControllerSpriteData[Joystick.BUTTON_L_STICK_D] = _ControllerSpriteData[Joystick.BUTTON_L_STICK_U]
_ControllerSpriteData[Joystick.BUTTON_L_STICK_LR] = {
  128,
  54,
  0,
  40,
  128,
  128
}
_ControllerSpriteData[Joystick.BUTTON_USE_MELEE] = {
  128,
  64,
  0,
  40,
  128,
  128
}
_ControllerSpriteData[Joystick.BUTTON_USE_RELOAD] = {
  128,
  64,
  0,
  40,
  128,
  128
}
_ControllerXboxSpriteData = {}
_ControllerXboxSpriteData[Joystick.BUTTON_PAD2_U] = {
  64,
  64,
  0,
  24,
  128,
  64
}
_ControllerXboxSpriteData[Joystick.BUTTON_PAD2_D] = _ControllerXboxSpriteData[Joystick.BUTTON_PAD2_U]
_ControllerXboxSpriteData[Joystick.BUTTON_PAD2_L] = _ControllerXboxSpriteData[Joystick.BUTTON_PAD2_U]
_ControllerXboxSpriteData[Joystick.BUTTON_PAD2_R] = _ControllerXboxSpriteData[Joystick.BUTTON_PAD2_U]
_ControllerXboxSpriteData[Joystick.BUTTON_L_STICK_L] = {
  116,
  128,
  0,
  0,
  512,
  128
}
_ControllerXboxSpriteData[Joystick.BUTTON_L_STICK_R] = _ControllerXboxSpriteData[Joystick.BUTTON_L_STICK_L]
_ControllerXboxSpriteData[Joystick.BUTTON_L_STICK_U] = {
  116,
  128,
  0,
  0,
  512,
  128
}
_ControllerXboxSpriteData[Joystick.BUTTON_L_STICK_D] = _ControllerXboxSpriteData[Joystick.BUTTON_L_STICK_U]
_ControllerXboxSpriteData[Joystick.BUTTON_L_STICK_LR] = {
  128,
  64,
  0,
  40,
  128,
  128
}
_ControllerXboxSpriteData[Joystick.BUTTON_USE_MELEE] = {
  128,
  64,
  0,
  40,
  128,
  128
}
_ControllerXboxSpriteData[Joystick.BUTTON_USE_RELOAD] = {
  128,
  64,
  0,
  40,
  128,
  128
}
