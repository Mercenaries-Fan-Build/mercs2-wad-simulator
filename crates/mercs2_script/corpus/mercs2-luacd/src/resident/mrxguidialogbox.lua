import("MrxGuiBase")
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
oSelectableList = {}

function DisplayDialogBox(uPlayerGuid, sMessage, tOptions, nDefaultCursorIndex, fCallback, tCallbackArgs, nXOffset, nYOffset, sHorizAnchor, sVertAnchor, bPause, nCancelOption)
  if "string" ~= type(sMessage) then
    return
  end
  if "table" ~= type(tOptions) then
    return
  end
  if table.getn(tOptions) < 1 then
    tOptions[1] = "[Generic.Ok]"
  end
  nDefaultCursorIndex = _ValidateParameter(nDefaultCursorIndex, "number", 1)
  fCallback = _ValidateParameter(fCallback, "function", nil)
  tCallbackArgs = _ValidateParameter(tCallbackArgs, "table", {})
  nCancelOption = _ValidateParameter(nCancelOption, "number", nil)
  if nil == bPause then
    bPause = true
  end
  local oBox = _BuildDialogBox(sMessage, tOptions, nDefaultCursorIndex, fCallback, tCallbackArgs, uPlayerGuid, nXOffset, nYOffset, sHorizAnchor, sVertAnchor, nCancelOption)
  oBox:SetOwner(uPlayerGuid)
  MrxGuiBase.GetControlFocus(oBox, bPause)
  oBox.Close = Close
  return oBox
end

function Close(oBox)
  MrxGuiBase.ReleaseControlFocus(oBox)
  MrxGuiBase.RemoveWidgetWithChildren(oBox)
  oBox:DeleteWithChildren()
end

function _BuildDialogBox(sMessage, tOptions, nDefaultCursorIndex, fCallback, tCallbackArgs, uPlayerGuid, nXOffset, nYOffset, sHorizAnchor, sVertAnchor, nCancelOption)
  local nStartX = 170
  local nStartY = 100
  local nWidth = 298
  local nBorder = 10
  local nYBorder = 20
  local nCurrentY = nYBorder + nStartY
  local oDialogBox = MrxGuiBase.ImageWidget:new()
  oDialogBox:SetLocation(nStartX, nStartY, nStartX + nWidth, 400)
  oDialogBox.BasicData.bContainer = true
  oDialogBox:SetOwner(uPlayerGuid)
  oDialogBox:SetVisible(false)
  oSelectableList = {}
  local oDialogBoxText = MrxGuiBase.TextWidget:new()
  oDialogBoxText:SetLocation(nStartX + nBorder, nStartY + nYBorder, nStartX + nWidth - nBorder, 390)
  oDialogBoxText:SetFont(_ksFont)
  oDialogBoxText:SetScale(_knScale)
  oDialogBoxText:SetColor(_knTextLitR, _knTextLitG, _knTextLitB)
  oDialogBoxText:SetText(sMessage)
  oDialogBoxText:Wrap()
  oDialogBoxText:SetOwner(uPlayerGuid)
  oDialogBoxText.ParentWidget = oDialogBox
  nCurrentY = nCurrentY + oDialogBoxText:GetHeight() + 4
  local oCursor = MrxGuiBase.ImageWidget:new()
  oCursor:SetColor(84, 79, 57)
  oCursor:SetTranslucency(205)
  oCursor:SetLocation(nStartX + nBorder, 1, nStartX + nWidth - nBorder, 2)
  oCursor:SetOwner(uPlayerGuid)
  oCursor.ParentWidget = oDialogBox
  oCursor.CustomData.nClosePoint = oCursor:AddAnimationPoint({y = 1, y2 = 1})
  oCursor.CustomData.nOpenPoint = oCursor:AddAnimationPoint({y = 1, y2 = 1})
  oCursor.CustomData.nPulseHighPoint = oCursor:AddAnimationPoint({TranslucencyLevel = 255})
  oCursor.CustomData.nPulseLowPoint = oCursor:AddAnimationPoint({TranslucencyLevel = 100})
  oCursor:SetIgnoresPause(true)
  local oDialogBoxOptions = MrxGuiBase.ImageWidget:new()
  oDialogBoxOptions:SetTranslucency(0)
  oDialogBoxOptions:SetLocation(nStartX + nBorder, nCurrentY, nStartX + nWidth - nBorder, nCurrentY + 200)
  oDialogBoxOptions:SetOwner(uPlayerGuid)
  oDialogBoxOptions.ParentWidget = oDialogBox
  local nIndent = 50
  local nIndex = 1
  while "string" == type(tOptions[nIndex]) do
    local oOptionText = MrxGuiBase.TextWidget:new()
    oOptionText:SetLocation(nStartX + nIndent, nCurrentY, nStartX + nWidth - nBorder, nCurrentY)
    oOptionText:SetFont(_ksFont)
    oOptionText:SetScale(_knScale)
    oOptionText:SetColor(_knTextR, _knTextG, _knTextB)
    oOptionText:SetText(tOptions[nIndex])
    oOptionText:SetHighlightable(1)
    table.insert(oSelectableList, oOptionText)
    oOptionText:Wrap()
    oOptionText:SetOwner(uPlayerGuid)
    oOptionText.ParentWidget = oDialogBoxOptions
    oDialogBoxOptions:AddChild(oOptionText)
    oOptionText.CustomData.nHeight = oOptionText:GetHeight()
    nIndex = nIndex + 1
    nCurrentY = nCurrentY + oOptionText.CustomData.nHeight + 2
  end
  if 1 == nIndex then
    oDialogBox:DeleteWithChildren()
    return nil
  end
  if not oDialogBoxOptions:GetChildren()[nDefaultCursorIndex] then
    nDefaultCursorIndex = 1
  end
  oDialogBox.CustomData.nSelectedIndex = nDefaultCursorIndex
  local oSelectedText = oDialogBoxOptions:GetChildren()[oDialogBox.CustomData.nSelectedIndex]
  local nX1, nY1 = oSelectedText:GetLocation()
  oCursor:SetLocation(nil, nY1, nil, nY1 + oSelectedText.CustomData.nHeight)
  oSelectedText:SetColor(_knTextLitR, _knTextLitG, _knTextLitB)
  local oCalloutText = MrxGuiBase.TextWidget:new()
  nCurrentY = nCurrentY + 8
  oCalloutText:SetLocation(nStartX + nBorder, nCurrentY, nStartX + nWidth - nBorder, nCurrentY)
  oCalloutText:SetFont(_ksFont)
  oCalloutText:SetScale(_knScale)
  oCalloutText:SetColor(_knTextR, _knTextG, _knTextB)
  oCalloutText:SetJustification("center")
  oCalloutText:SetText("[move] [PDA.Common.MoveSelection]  [confirm] [Generic.Confirm]")
  oCalloutText:SetOwner(uPlayerGuid)
  oCalloutText.ParentWidget = oDialogBox
  oCalloutText.CustomData.nHeight = oCalloutText:GetHeight()
  nCurrentY = nCurrentY + oCalloutText.CustomData.nHeight - 8
  local tBg = {}
  local nStartU = 0
  local nEndU = 0.8730469
  local nTopStartV = 0
  local nTopEndV = 0.078125
  local nMidStartV = 0.083984375
  local nMidEndV = 0.17382812
  local nBottomStartV = 0.1796875
  local nBottomEndV = 0.2734375
  local nScaleFactor = 0.6666667
  local nPieceWidth = 447 * nScaleFactor
  local nTopHeight = 48 * nScaleFactor
  local nMidHeight = 46 * nScaleFactor
  local nBottomHeight = 48 * nScaleFactor
  local nHeight = nCurrentY - nStartY
  local sTexture = "global_gui_hud02"
  local nCurrentY = nStartY
  local n = 2
  tBg[1] = MrxGuiBase.ImageWidget:new()
  tBg[1]:SetTexture(sTexture)
  tBg[1]:SetLocation(nStartX, nCurrentY, nStartX + nPieceWidth, nCurrentY + nTopHeight)
  tBg[1]:SetTextureCoordinates(nStartU, nBottomEndV, nEndU, nBottomStartV)
  tBg[1]:SetOwner(uPlayerGuid)
  nHeight = nHeight - nTopHeight
  nCurrentY = nCurrentY + nTopHeight
  nMidHeight = nHeight - (nBottomHeight - 20 * nScaleFactor) + nYBorder
  tBg[n] = MrxGuiBase.ImageWidget:new()
  tBg[n]:SetTexture(sTexture)
  tBg[n]:SetLocation(nStartX, nCurrentY, nStartX + nPieceWidth, nCurrentY + nMidHeight)
  tBg[n]:SetTextureCoordinates(nStartU, nMidStartV, nEndU, nMidEndV)
  tBg[n]:SetOwner(uPlayerGuid)
  nHeight = nHeight - nMidHeight
  nCurrentY = nCurrentY + nMidHeight
  n = n + 1
  tBg[n] = MrxGuiBase.ImageWidget:new()
  tBg[n]:SetTexture(sTexture)
  tBg[n]:SetLocation(nStartX, nCurrentY, nStartX + nPieceWidth, nCurrentY + nBottomHeight)
  tBg[n]:SetTextureCoordinates(nStartU, nBottomStartV, nEndU, nBottomEndV)
  tBg[n]:SetOwner(uPlayerGuid)
  nCurrentY = nCurrentY + nBottomHeight
  n = 1
  while tBg[n] do
    oDialogBox:AddChild(tBg[n])
    n = n + 1
  end
  oDialogBox:AddChild(oDialogBoxText)
  oDialogBox:AddChild(oCursor)
  oDialogBox:AddChild(oDialogBoxOptions)
  oDialogBox:AddChild(oCalloutText)
  oDialogBox.CustomData.oCursor = oCursor
  oDialogBox.CustomData.oOptions = oDialogBoxOptions
  oDialogBox.CustomData.fCallback = fCallback
  oDialogBox.CustomData.tCallbackArgs = tCallbackArgs
  oDialogBox.CustomData.nCancelOption = nCancelOption
  local nRealHeight = nCurrentY - nStartY
  nStartY = 240 - nRealHeight / 2
  if nStartY < 0 then
    nStartY = 0
  end
  oDialogBox:SetLocation(nStartX, nStartY)
  oDialogBox:SetCoordinates(nStartX, nStartY, nStartX + nWidth, nStartY + nRealHeight)
  if nXOffset and nYOffset and sHorizAnchor and sVertAnchor then
    nHeight = nRealHeight
    local nNewX, nNewY
    if "left" == sHorizAnchor then
      if nXOffset < 48 then
        nXOffset = 48
      end
      nNewX = nXOffset
    elseif "right" == sHorizAnchor then
      if nXOffset < 48 then
        nXOffset = 48
      end
      nNewX = 640 - nWidth - nXOffset
    elseif "center" == sHorizAnchor then
      nNewX = nXOffset - nWidth * 0.5 + 320
      if nNewX < 48 then
        nNewX = 48
      elseif nNewX > 592 - nWidth then
        nNewX = 592 - nWidth
      end
    end
    if "top" == sVertAnchor then
      if nYOffset < 36 then
        nYOffset = 36
      end
      nNewY = nYOffset
    elseif "bottom" == sVertAnchor then
      if nYOffset < 36 then
        nYOffset = 36
      end
      nNewY = 480 - nHeight - nYOffset
    elseif "center" == sVertAnchor then
      nNewY = nYOffset - nHeight * 0.5 + 240
      if nNewY < 36 then
        nNewY = 36
      elseif nNewY > 444 - nHeight then
        nNewY = 444 - nHeight
      end
    end
    oDialogBox:SetAnchoring(sHorizAnchor, sVertAnchor)
    oDialogBox:SetLocation(nNewX, nNewY)
  end
  MrxGuiBase.AddWidgetWithChildren(oDialogBox)
  oDialogBox:SetEventHandler("ControllerInput", _HandleInputEvent)
  oDialogBox:SetEventHandler("OnMouseMove", _HandleDialogUpdate)
  oDialogBox._ChangeSelection = _ChangeSelection
  Pulse(oCursor)
  return oDialogBox
end

_knPulseTime = 0.5

function Pulse(oWidget)
  local nAlpha = oWidget:GetTranslucency()
  if oWidget.CustomData.bRising then
    local nStartTime = (255 - nAlpha) / 255 * _knPulseTime
    oWidget:AnimateToPoint(oWidget.CustomData.nPulseHighPoint, nStartTime, true, _LoopToLow)
  else
    local nStartTime = (nAlpha - 100) / 255 * _knPulseTime
    oWidget:AnimateToPoint(oWidget.CustomData.nPulseLowPoint, nStartTime, true, _LoopToHigh)
  end
end

function _LoopToHigh(oWidget)
  oWidget.CustomData.bRising = true
  oWidget:AnimateToPoint(oWidget.CustomData.nPulseHighPoint, _knPulseTime, true, _LoopToLow)
end

function _LoopToLow(oWidget)
  oWidget.CustomData.bRising = false
  oWidget:AnimateToPoint(oWidget.CustomData.nPulseLowPoint, _knPulseTime, true, _LoopToHigh)
end

function HaltPulse(oWidget)
  oWidget.CustomData.bRising = false
  if bImmediate then
    local nAlpha = oWidget:GetTranslucency()
    local nStartTime = (255 - nAlpha) / 255 * _knPulseTime
    oWidget:AnimateToPoint(oWidget.CustomData.nPulseHighPoint, nStartTime, true)
  else
    oWidget:AnimateToPoint(oWidget.CustomData.nPulseHighPoint, 0, true)
  end
end

function _ChangeSelection(oDialogBox, bUp)
  local nIncrement = 1
  if bUp then
    nIncrement = -1
  end
  local tChildren = oDialogBox.CustomData.oOptions:GetChildren()
  local oPrevOption = tChildren[oDialogBox.CustomData.nSelectedIndex]
  oPrevOption:SetColor(_knTextR, _knTextG, _knTextB)
  oDialogBox.CustomData.nSelectedIndex = oDialogBox.CustomData.nSelectedIndex + nIncrement
  if bUp then
    if 1 > oDialogBox.CustomData.nSelectedIndex then
      oDialogBox.CustomData.nSelectedIndex = #tChildren
    end
  elseif not tChildren[oDialogBox.CustomData.nSelectedIndex] then
    oDialogBox.CustomData.nSelectedIndex = 1
  end
  local oCurrentOption = tChildren[oDialogBox.CustomData.nSelectedIndex]
  local nOldX1, nOldY1 = oPrevOption:GetLocation()
  local nOldY2 = nOldY1 + oPrevOption.CustomData.nHeight
  local nDestX1, nDestY1 = oCurrentOption:GetLocation()
  local nDestY2 = nDestY1 + oCurrentOption.CustomData.nHeight
  local oCursor = oDialogBox.CustomData.oCursor
  local nCurX1, nCurY1, nCurX2, nCurY2 = oCursor:GetLocation()
  local nCloseY = (nCurY1 + nCurY2) * 0.5
  oCursor:SetAnimationPoint(oCursor.CustomData.nClosePoint, {y = nCloseY, y2 = nCloseY})
  oCursor:SetAnimationPoint(oCursor.CustomData.nOpenPoint, {y = nDestY1, y2 = nDestY2})
  oCursor:SetLocation(nil, nOldY1, nil, nOldY2)
  oCursor:AnimateToPoint(oCursor.CustomData.nClosePoint, 0.075, true, _CompleteAnimation, {
    nDestY1,
    nDestY2,
    oCurrentOption
  })
end

function _CompleteAnimation(oCursor, nY1, nY2, oCurrentOption)
  local nMidY = (nY1 + nY2) * 0.5
  oCursor:SetLocation(nil, nMidY, nil, nMidY)
  oCursor:SetAnimationPoint(oCursor.CustomData.nOpenPoint, {y = nY1, y2 = nY2})
  oCursor:AnimateToPoint(oCursor.CustomData.nOpenPoint, 0.075, true, Pulse)
  oCurrentOption:SetColor(_knTextLitR, _knTextLitG, _knTextLitB)
end

function _HandleInputEvent(oDialogBox, tEvent)
  if MrxGuiBase.Joystick.BUTTON_PAD1_D == tEvent.ButtonPress or MrxGuiBase.Joystick.BUTTON_L_STICK_D == tEvent.ButtonPress then
    oDialogBox:_ChangeSelection(false)
    Sound.CueSound(0, _ksChangeSound)
  elseif MrxGuiBase.Joystick.BUTTON_PAD1_U == tEvent.ButtonPress or MrxGuiBase.Joystick.BUTTON_L_STICK_U == tEvent.ButtonPress then
    oDialogBox:_ChangeSelection(true)
    Sound.CueSound(0, _ksChangeSound)
  elseif MrxGuiBase.Joystick.BUTTON_PAD2_D == tEvent.ButtonPress then
    _CloseAndCallCallback(oDialogBox, oDialogBox.CustomData.nSelectedIndex)
    Sound.CueSound(0, _ksAcceptSound)
  elseif MrxGuiBase.Joystick.BUTTON_PAD2_R == tEvent.ButtonPress and oDialogBox.CustomData.nCancelOption then
    _CloseAndCallCallback(oDialogBox, oDialogBox.CustomData.nCancelOption)
    Sound.CueSound(0, _ksCancelSound)
  end
end

function _HandleDialogUpdate(oDialogBox, tEvent)
  local nId = _GuiInternal.GetWidgetHighlightId()
  local downId = _GuiInternal.GetWidgetDownId()
  local selInx = oDialogBox.CustomData.nSelectedIndex
  if nId ~= 0 then
    local tChildren = oDialogBox.CustomData.oOptions:GetChildren()
    local nId2 = 0
    for nIndex = 1, #tChildren do
      local oOption = tChildren[nIndex]
      nId2 = oOption.BasicData.uId
      if nId == nId2 then
        selInx = nIndex
        Debug.Printf("selInx: " .. tostring(nIndex) .. " ")
      end
    end
    if oDialogBox.CustomData.nSelectedIndex ~= selInx then
      local oPrevOption = tChildren[oDialogBox.CustomData.nSelectedIndex]
      oPrevOption:SetColor(_knTextR, _knTextG, _knTextB)
      oDialogBox.CustomData.nSelectedIndex = selInx
      local oCurrentOption = tChildren[oDialogBox.CustomData.nSelectedIndex]
      local nOldX1, nOldY1 = oPrevOption:GetLocation()
      local nOldY2 = nOldY1 + oPrevOption.CustomData.nHeight
      local nDestX1, nDestY1 = oCurrentOption:GetLocation()
      local nDestY2 = nDestY1 + oCurrentOption.CustomData.nHeight
      local oCursor = oDialogBox.CustomData.oCursor
      local nCurX1, nCurY1, nCurX2, nCurY2 = oCursor:GetLocation()
      local nCloseY = (nCurY1 + nCurY2) * 0.5
      oCursor:SetAnimationPoint(oCursor.CustomData.nClosePoint, {y = nCloseY, y2 = nCloseY})
      oCursor:SetAnimationPoint(oCursor.CustomData.nOpenPoint, {y = nDestY1, y2 = nDestY2})
      oCursor:SetLocation(nil, nOldY1, nil, nOldY2)
      oCursor:AnimateToPoint(oCursor.CustomData.nClosePoint, 0.075, true, _CompleteAnimation, {
        nDestY1,
        nDestY2,
        oCurrentOption
      })
    end
  end
end

function _CloseAndCallCallback(oDialogBox, nSelectedIndex)
  MrxGuiBase.ReleaseControlFocus(oDialogBox)
  local fCallback = oDialogBox.CustomData.fCallback
  local tCallbackArgs = oDialogBox.CustomData.tCallbackArgs
  oDialogBox:Close()
  if fCallback then
    table.insert(tCallbackArgs, nSelectedIndex)
    fCallback(unpack(tCallbackArgs))
  end
end

local nScrollBarWidth = 10
local nMaxTextHeight = 200
local nScrollSpeed = 100

function DisplayScrollingDialogBox(uPlayer, sText, fCallback, tCallbackData, bDisplayWager, sAcceptString, sDeclineString, sWagerString)
  if "userdata" ~= type(uPlayer) then
    return
  end
  if "string" ~= type(sText) then
    return
  end
  local oBox = _BuildScrollingDialogBox(uPlayer, sText, bDisplayWager, fCallback, tCallbackData, sAcceptString, sDeclineString, sWagerString)
  local nX1, nY1, nX2, nY2 = oBox:GetLocation()
  local nX = 320 - (nX2 - nX1) * 0.5
  local nY = 240 - (nY2 - nY1) * 0.5
  oBox:SetLocation(nX, nY)
  oBox:SetAnchoring("center", "center")
  MrxGuiBase.AddWidgetWithChildren(oBox)
  MrxGuiBase.GetControlFocus(oBox, false)
  return oBox
end

function _BuildScrollingDialogBox(uPlayer, sText, bDisplayWager, fCallback, tCallbackData, sAcceptString, sDeclineString, sWagerString)
  local nWidth = 298
  local nMargin = 10
  local nYMargin = 30
  local oText = MrxGuiBase.TextWidget:new()
  oText:SetFont("english_18")
  oText:SetText(sText)
  oText:SetLocation(0, 0, nWidth - nMargin - nMargin, nMaxTextHeight)
  oText:Wrap()
  oText:SetOwner(uPlayer)
  local oScroll
  local nActualTextHeight = nMaxTextHeight
  if oText:GetHeight() > nMaxTextHeight then
    oScroll = _CreateScrollableWindow(uPlayer, oText, 0, 0, nWidth - nMargin * 2 - nScrollBarWidth, nMaxTextHeight)
    oText:delete()
    oText = nil
  else
    nActualTextHeight = oText:GetHeight()
  end
  local oOptions = MrxGuiBase.Widget:new()
  local nOptionsHeight = 50
  local nOptions = 2
  if bDisplayWager then
    nOptions = 3
  end
  local nOptionWidth = (nWidth - nMargin * 2) / nOptions
  oOptions:SetLocation(0, 0, nOptionWidth * nOptions, nOptionsHeight)
  oOptions:SetOwner(uPlayer)
  local oAcceptOption = MrxGuiBase.TextWidget:new()
  local oDeclineOption = MrxGuiBase.TextWidget:new()
  oSelectableList = {}
  oAcceptOption:SetFont("english_18")
  oAcceptOption:SetText(sAcceptString or "[Generic.Accept]")
  oAcceptOption:SetOwner(uPlayer)
  oAcceptOption:SetJustification("center")
  local oButton = MrxGuiBase.ImageWidget:new()
  oButton:SetLocation(2, 0, nOptionWidth - 2, nOptionsHeight)
  oButton:SetColor(255, 0, 0, 0)
  oButton:SetHighlightable(1)
  oOptions:AddChild(oButton)
  table.insert(oSelectableList, oButton)
  local nLineHeight = oAcceptOption:GetHeight()
  oAcceptOption:SetLocation(0, nOptionsHeight / 2 - nLineHeight / 2, nOptionWidth, nOptionsHeight / 2 + nLineHeight / 2)
  oAcceptOption:Wrap()
  nLineHeight = oAcceptOption:GetHeight()
  oAcceptOption:SetLocation(0, nOptionsHeight / 2 - nLineHeight / 2, nOptionWidth, nOptionsHeight / 2 + nLineHeight / 2)
  _BuildStrokes(oAcceptOption, 2, 0, nOptionWidth - 2, nOptionsHeight)
  oDeclineOption:SetFont("english_18")
  oDeclineOption:SetText(sDeclineString or "[Generic.Decline]")
  oDeclineOption:SetOwner(uPlayer)
  oDeclineOption:SetJustification("center")
  local oButton2 = MrxGuiBase.ImageWidget:new()
  oButton2:SetLocation(nOptionWidth + 2, 0, nOptionWidth * 2 - 2, nOptionsHeight)
  oButton2:SetColor(255, 0, 0, 0)
  oButton2:SetHighlightable(1)
  oOptions:AddChild(oButton2)
  table.insert(oSelectableList, oButton2)
  oDeclineOption:SetLocation(nOptionWidth, nOptionsHeight / 2 - nLineHeight / 2, nOptionWidth * 2, nOptionsHeight / 2 + nLineHeight / 2)
  oDeclineOption:Wrap()
  nLineHeight = oDeclineOption:GetHeight()
  oDeclineOption:SetLocation(nOptionWidth, nOptionsHeight / 2 - nLineHeight / 2, nOptionWidth * 2, nOptionsHeight / 2 + nLineHeight / 2)
  _BuildStrokes(oDeclineOption, nOptionWidth + 2, 0, nOptionWidth * 2 - 2, nOptionsHeight)
  local oWagerOption
  if bDisplayWager then
    oWagerOption = MrxGuiBase.TextWidget:new()
    oWagerOption:SetFont("english_18")
    oWagerOption:SetText(sWagerString or "[Briefing.ChangeWager]")
    oWagerOption:SetJustification("center")
    local oButton3 = MrxGuiBase.ImageWidget:new()
    oButton3:SetLocation(nOptionWidth * 2 + 2, 0, nOptionWidth * 3 - 2, nOptionsHeight)
    oButton3:SetColor(255, 0, 0, 0)
    oButton3:SetHighlightable(1)
    oOptions:AddChild(oButton3)
    table.insert(oSelectableList, oButton3)
    oWagerOption:SetOwner(uPlayer)
    local nWagerHeight = oWagerOption:GetHeight()
    oWagerOption:SetLocation(nOptionWidth * 2, nOptionsHeight / 2 - nWagerHeight / 2, nOptionWidth * 3, nOptionsHeight / 2 + nWagerHeight / 2)
    oWagerOption:Wrap()
    nWagerHeight = oWagerOption:GetHeight()
    oWagerOption:SetLocation(nOptionWidth * 2, nOptionsHeight / 2 - nWagerHeight / 2, nOptionWidth * 3, nOptionsHeight / 2 + nWagerHeight / 2)
    _BuildStrokes(oWagerOption, nOptionWidth * 2 + 2, 0, nOptionWidth * 3 - 2, nOptionsHeight)
  end
  local oCursor = MrxGuiBase.ImageWidget:new()
  oCursor:SetColor(84, 79, 57)
  oCursor:SetOwner(uPlayer)
  oCursor.CustomData.nClosePoint = oCursor:AddAnimationPoint({y = 1, y2 = 1})
  oCursor.CustomData.nOpenPoint = oCursor:AddAnimationPoint({y = 1, y2 = 1})
  oCursor.CustomData.nPulseHighPoint = oCursor:AddAnimationPoint({TranslucencyLevel = 255})
  oCursor.CustomData.nPulseLowPoint = oCursor:AddAnimationPoint({TranslucencyLevel = 100})
  oCursor:SetIgnoresPause(true)
  oCursor:SetLocation(0, 0, nOptionWidth, nOptionsHeight)
  oOptions.CustomData.tOptions = {
    oAcceptOption,
    oDeclineOption,
    oWagerOption
  }
  oOptions.CustomData.oCursor = oCursor
  oOptions.SetOption = _SetScrollOption
  oOptions:AddChild(oCursor)
  oOptions:AddChild(oAcceptOption)
  oOptions:AddChild(oDeclineOption)
  if oWagerOption then
    oOptions:AddChild(oWagerOption)
  end
  local oCalloutText = MrxGuiBase.TextWidget:new()
  oCalloutText:SetLocation(0, 0, nWidth - nMargin * 2, 10)
  oCalloutText:SetFont(_ksFont)
  oCalloutText:SetScale(_knScale)
  oCalloutText:SetColor(_knTextR, _knTextG, _knTextB)
  oCalloutText:SetJustification("center")
  oCalloutText:SetText("[move] [PDA.Common.MoveSelection]  [confirm] [Generic.Confirm]")
  oCalloutText:SetOwner(uPlayerGuid)
  local nCalloutHeight = oCalloutText:GetHeight()
  local oBg = MrxGuiBase.Widget:new()
  oBg:SetOwner(uPlayer)
  local oBgTop = MrxGuiBase.ImageWidget:new()
  local oBgMid = MrxGuiBase.ImageWidget:new()
  local oBgBot = MrxGuiBase.ImageWidget:new()
  oBgTop:SetOwner(uPlayer)
  oBgMid:SetOwner(uPlayer)
  oBgBot:SetOwner(uPlayer)
  local nScaleFactor = 0.6666667
  local nBgWidth = 447 * nScaleFactor
  local nEndHeight = 48 * nScaleFactor
  oBgTop:SetLocation(0, 0, nBgWidth, nEndHeight)
  oBgTop:SetTexture("global_gui_hud02")
  oBgTop:SetTextureCoordinates(0, 0.2734375, 0.8730469, 0.1796875)
  local nMidEnd = nActualTextHeight + nOptionsHeight + nYMargin * 4 - nEndHeight
  oBgMid:SetLocation(0, 48 * nScaleFactor, nBgWidth, nMidEnd)
  oBgMid:SetTexture("global_gui_hud02")
  oBgMid:SetTextureCoordinates(0, 0.083984375, 0.8730469, 0.17382812)
  oBgBot:SetLocation(0, nMidEnd, nBgWidth, nMidEnd + nEndHeight)
  oBgBot:SetTexture("global_gui_hud02")
  oBgBot:SetTextureCoordinates(0, 0.1796875, 0.8730469, 0.2734375)
  oBg:SetLocation(0, 0, nBgWidth, nMidEnd + nEndHeight)
  oBg:AddChild(oBgTop)
  oBg:AddChild(oBgMid)
  oBg:AddChild(oBgBot)
  local oBox = MrxGuiBase.Widget:new()
  oBox:SetLocation(0, 0, nWidth, nActualTextHeight + nOptionsHeight + nYMargin * 4)
  oBox:SetOwner(uPlayer)
  oBox:AddChild(oBg)
  if oText then
    oBox:AddChild(oText)
  end
  if oScroll then
    oBox:AddChild(oScroll)
  end
  oBox:AddChild(oOptions)
  oBox:AddChild(oCalloutText)
  oBox.CustomData.oOptions = oOptions
  oBox.CustomData.oScroll = oScroll
  oBox.CustomData.oCursor = oCursor
  oBox.CustomData.UpArrowHighlight = 0
  oBox.CustomData.DownArrowHighlight = 0
  if oScroll then
    oScroll:SetLocation(nMargin, nYMargin)
  else
    oText:SetLocation(nMargin, nYMargin)
  end
  oOptions:SetLocation(nMargin, nActualTextHeight + nYMargin * 1.75)
  oCalloutText:SetLocation(nMargin, nActualTextHeight + nYMargin * 2.25 + nOptionsHeight)
  oBox.Close = CloseScrollingBox
  oBox.CustomData.nScroll = 0
  oBox.CustomData.nPadScroll = 0
  oBox.CustomData.nAnalogScroll = 0
  oBox.CustomData.nNumOptions = nOptions
  oBox.CustomData.nSelected = 1
  oBox.CustomData.fCallback = fCallback
  oBox.CustomData.tCallbackData = tCallbackData
  oBox:SetEventHandler("GuiUpdate", _HandleScrollUpdate)
  oBox:SetEventHandler("OnMouseMove", _HandleMouseUpdate)
  oBox:SetEventHandler("ControllerInput", _HandleScrollInput)
  Pulse(oCursor)
  return oBox
end

function _BuildStrokes(oWidget, nX1, nY1, nX2, nY2)
  local nR = 128
  local nG = 128
  local nB = 96
  local nA = 255
  local nW = 1
  local oStroke1 = MrxGuiBase.ImageWidget:new()
  oStroke1:SetColor(nR, nG, nB, nA)
  oStroke1:SetLocation(nX1, nY1, nX1 + nW, nY2)
  local oStroke2 = MrxGuiBase.ImageWidget:new()
  oStroke2:SetColor(nR, nG, nB, nA)
  oStroke2:SetLocation(nX1, nY1, nX2, nY1 + nW)
  local oStroke3 = MrxGuiBase.ImageWidget:new()
  oStroke3:SetColor(nR, nG, nB, nA)
  oStroke3:SetLocation(nX2 - nW, nY1, nX2, nY2)
  local oStroke4 = MrxGuiBase.ImageWidget:new()
  oStroke4:SetColor(nR, nG, nB, nA)
  oStroke4:SetLocation(nX1, nY2 - nW, nX2, nY2)
  oWidget:AddChild(oStroke1)
  oWidget:AddChild(oStroke2)
  oWidget:AddChild(oStroke3)
  oWidget:AddChild(oStroke4)
end

function _HandleScrollUpdate(oBox, nDeltaTime)
  if oBox.CustomData.oScroll and math.abs(oBox.CustomData.nScroll) > 0.5 then
    local nOffset = nDeltaTime * nScrollSpeed * oBox.CustomData.nScroll
    oBox.CustomData.oScroll:OffsetText(nOffset)
  end
end

function _HandleMouseUpdate(oBox, nDeltaTime)
  local nId = _GuiInternal.GetWidgetHighlightId()
  local downId = _GuiInternal.GetWidgetDownId()
  local selInx = oBox.CustomData.nSelected
  if nId ~= 0 then
    local tChildren = oSelectableList
    local nId2 = 0
    local mod = 0
    if type(tChildren) ~= "nil" then
      for nIndex = 1, #tChildren do
        local oOption = tChildren[nIndex]
        nId2 = oOption.BasicData.uId
        if nId == nId2 then
          selInx = nIndex + mod
          Debug.Printf("selInx: " .. tostring(nIndex) .. " ")
        end
      end
    end
    if oBox.CustomData.nSelected ~= selInx then
      oBox.CustomData.nSelected = selInx
      oBox.CustomData.oOptions:SetOption(oBox.CustomData.nSelected)
    end
    if nId == oBox.CustomData.oScroll.CustomData.oUpArrow.BasicData.uId then
      oBox.CustomData.UpArrowHighlight = 1
      oBox.CustomData.DownArrowHighlight = 0
    elseif nId == oBox.CustomData.oScroll.CustomData.oDownArrow.BasicData.uId then
      oBox.CustomData.UpArrowHighlight = 0
      oBox.CustomData.DownArrowHighlight = 1
    else
      oBox.CustomData.UpArrowHighlight = 0
      oBox.CustomData.DownArrowHighlight = 0
    end
  else
    oBox.CustomData.UpArrowHighlight = 0
    oBox.CustomData.DownArrowHighlight = 0
  end
end

function _HandleScrollInput(oBox, tEvent)
  if MrxGuiBase.Joystick.BUTTON_PAD1_D == tEvent.ButtonPress then
    oBox.CustomData.nPadScroll = -1
  elseif MrxGuiBase.Joystick.BUTTON_PAD1_U == tEvent.ButtonPress then
    oBox.CustomData.nPadScroll = 1
  else
    oBox.CustomData.nPadScroll = 0
  end
  if -1 == oBox.CustomData.nAnalogScroll then
    if MrxGuiBase.Joystick.BUTTON_L_STICK_D == tEvent.ButtonReleased then
      oBox.CustomData.nAnalogScroll = 0
    end
  elseif MrxGuiBase.Joystick.BUTTON_L_STICK_D == tEvent.ButtonPress then
    oBox.CustomData.nAnalogScroll = -1
  end
  if 1 == oBox.CustomData.nAnalogScroll then
    if MrxGuiBase.Joystick.BUTTON_L_STICK_U == tEvent.ButtonReleased then
      oBox.CustomData.nAnalogScroll = 0
    end
  elseif MrxGuiBase.Joystick.BUTTON_L_STICK_U == tEvent.ButtonPress then
    oBox.CustomData.nAnalogScroll = 1
  end
  oBox.CustomData.nScroll = oBox.CustomData.nPadScroll + oBox.CustomData.nAnalogScroll
  local nSelectChange = 0
  if MrxGuiBase.Joystick.BUTTON_PAD1_L == tEvent.ButtonPress or MrxGuiBase.Joystick.BUTTON_L_STICK_L == tEvent.ButtonPress then
    nSelectChange = -1
  elseif MrxGuiBase.Joystick.BUTTON_PAD1_R == tEvent.ButtonPress or MrxGuiBase.Joystick.BUTTON_L_STICK_R == tEvent.ButtonPress then
    nSelectChange = 1
  end
  if math.abs(nSelectChange) > 0.5 then
    oBox.CustomData.nSelected = oBox.CustomData.nSelected + nSelectChange
    while oBox.CustomData.nSelected > oBox.CustomData.nNumOptions do
      oBox.CustomData.nSelected = oBox.CustomData.nSelected - oBox.CustomData.nNumOptions
    end
    while 1 > oBox.CustomData.nSelected do
      oBox.CustomData.nSelected = oBox.CustomData.nSelected + oBox.CustomData.nNumOptions
    end
    Sound.CueSound(0, _ksChangeSound)
    oBox.CustomData.oOptions:SetOption(oBox.CustomData.nSelected)
  end
  if MrxGuiBase.Joystick.BUTTON_PAD2_D == tEvent.ButtonPress then
    if oBox.CustomData.UpArrowHighlight == 1 or oBox.CustomData.DownArrowHighlight == 1 then
      if oBox.CustomData.UpArrowHighlight == 1 then
        oBox.CustomData.nScroll = 1
      elseif oBox.CustomData.DownArrowHighlight == 1 then
        oBox.CustomData.nScroll = -1
      end
    else
      local fCallback = oBox.CustomData.fCallback
      local tData = oBox.CustomData.tCallbackData
      local nSelected = oBox.CustomData.nSelected
      oBox:Close()
      Sound.CueSound(0, _ksAcceptSound)
      _CallScrollBoxCallback(fCallback, tData, nSelected)
    end
  elseif MrxGuiBase.Joystick.BUTTON_PAD2_R == tEvent.ButtonPress then
    local fCallback = oBox.CustomData.fCallback
    local tData = oBox.CustomData.tCallbackData
    oBox:Close()
    Sound.CueSound(0, _ksCancelSound)
    _CallScrollBoxCallback(fCallback, tData, 2)
  end
end

function _CallScrollBoxCallback(fCallback, tData, n)
  if fCallback then
    tData = tData or {}
    table.insert(tData, n)
    fCallback(unpack(tData))
  end
end

function CloseScrollingBox(oBox)
  MrxGuiBase.ReleaseControlFocus(oBox)
  MrxGuiBase.RemoveWidgetWithChildren(oBox)
  oBox:DeleteWithChildren()
end

function _SetScrollOption(oOptions, nOption)
  if not oOptions.CustomData.tOptions[nOption] then
    return
  end
  local nX1, nUnused1, nX2 = oOptions.CustomData.tOptions[nOption]:GetLocation()
  local nUnused2, nY1, nUnused3, nY2 = oOptions:GetLocation()
  oOptions.CustomData.nSelectedIndex = nOption
  local oCursor = oOptions.CustomData.oCursor
  local nCloseY = (nY1 + nY2) * 0.5
  oCursor:SetAnimationPoint(oCursor.CustomData.nClosePoint, {y = nCloseY, y2 = nCloseY})
  oCursor:SetAnimationPoint(oCursor.CustomData.nOpenPoint, {y = nY1, y2 = nY2})
  oCursor:SetLocation(nil, nY1, nil, nY2)
  oCursor:AnimateToPoint(oCursor.CustomData.nClosePoint, 0.075, true, _CompleteScrollAnimation, {nX1, nX2})
end

function _CompleteScrollAnimation(oCursor, nX1, nX2)
  oCursor:SetLocation(nX1, nil, nX2, nil)
  oCursor:AnimateToPoint(oCursor.CustomData.nOpenPoint, 0.075, true, Pulse)
end

function _CreateScrollableWindow(uPlayer, oTextWidget, nX, nY, nWidth, nHeight)
  local oScroll = MrxGuiBase.Widget:new()
  oScroll:SetLocation(0, 0, nWidth, nHeight)
  local nTextHeight = oTextWidget:GetHeight()
  local tTextLines = oTextWidget:SplitIntoLines()
  local nLineHeight = nTextHeight / #tTextLines
  local oTextContainer = MrxGuiBase.Widget:new()
  oTextContainer:SetLocation(0, 0, nWidth, nTextHeight)
  oTextContainer.CustomData.nLineHeight = nLineHeight
  local nCurrentY = 0
  for n, oTextLine in pairs(tTextLines) do
    oTextLine:SetLocation(0, nCurrentY)
    oTextLine:SetOwner(uPlayer)
    oTextContainer:AddChild(oTextLine)
    nCurrentY = nCurrentY + nLineHeight
  end
  oTextContainer.CustomData.tLines = tTextLines
  oTextContainer.CustomData.nMaxHeight = nHeight
  oTextContainer.UpdateAlpha = _UpdateTextAlpha
  oTextContainer.ParentWidget = oScroll
  oTextContainer:SetOwner(uPlayer)
  oTextContainer:UpdateAlpha()
  local oBarBg = MrxGuiBase.ImageWidget:new()
  local oBar = MrxGuiBase.ImageWidget:new()
  local nBarTotalHeight = nHeight - nScrollBarWidth * 2 - 2
  local nBarHeight = nBarTotalHeight * (nHeight / nTextHeight)
  oBarBg:SetColor(64, 64, 48)
  oBar:SetColor(128, 128, 96)
  oBarBg:SetLocation(nX + nWidth, nY, nX + nWidth + nScrollBarWidth, nY + nHeight)
  oBar:SetLocation(nWidth, nScrollBarWidth + 1, nWidth + nScrollBarWidth, nBarHeight + nScrollBarWidth + 1)
  local oUpArrow = MrxGuiBase.ImageWidget:new()
  oUpArrow:SetTexture("global_gui_hud02")
  oUpArrow:SetTextureCoordinates(0.001953, 0.947266, 0.162109, 0.986328)
  oUpArrow:SetLocation(0 + nWidth, 0, 0 + nWidth + nScrollBarWidth, nScrollBarWidth)
  oUpArrow:SetHighlightable(1)
  oScroll.CustomData.oUpArrow = oUpArrow
  local oDownArrow = MrxGuiBase.ImageWidget:new()
  oDownArrow:SetTexture("global_gui_hud02")
  oDownArrow:SetTextureCoordinates(0.001953, 0.986328, 0.162109, 0.947266)
  oDownArrow:SetLocation(nWidth, nHeight - nScrollBarWidth, nWidth + nScrollBarWidth, nHeight)
  oDownArrow:SetHighlightable(1)
  oScroll.CustomData.oDownArrow = oDownArrow
  oBar.CustomData.nHeight = nBarHeight
  oBar.CustomData.nTotalHeight = nBarTotalHeight
  oScroll:AddChild(oBarBg)
  oScroll:AddChild(oBar)
  oScroll:AddChild(oUpArrow)
  oScroll:AddChild(oDownArrow)
  oScroll:AddChild(oTextContainer)
  oScroll.CustomData.oBarBg = oBarBg
  oScroll.CustomData.oBar = oBar
  oScroll.CustomData.oTextContainer = oTextContainer
  oScroll.CustomData.nHeight = nHeight
  oScroll.CustomData.nTextHeight = nTextHeight
  oScroll.CustomData.nOffset = 0
  oScroll.OffsetText = _OffsetText
  oScroll:SetLocation(nX, nY)
  return oScroll
end

function _OffsetText(oScroll, nOffset)
  local nX, nY = oScroll:GetLocation()
  oScroll.CustomData.nOffset = oScroll.CustomData.nOffset + nOffset
  oScroll.CustomData.nOffset = _Clamp(oScroll.CustomData.nOffset, oScroll.CustomData.nHeight - oScroll.CustomData.nTextHeight, 0)
  oScroll.CustomData.oTextContainer:SetLocation(nX, nY + oScroll.CustomData.nOffset)
  oScroll.CustomData.oTextContainer:UpdateAlpha()
  local nBarX, nBarY = oScroll.CustomData.oBarBg:GetLocation()
  oScroll.CustomData.oBar:SetLocation(nBarX, nBarY + nScrollBarWidth + 1 - oScroll.CustomData.nOffset / oScroll.CustomData.nTextHeight * oScroll.CustomData.oBar.CustomData.nTotalHeight)
end

function _Clamp(n, lo, hi)
  return math.max(math.min(n, hi), lo)
end

function _UpdateTextAlpha(oTextContainer)
  local nMargin = oTextContainer.CustomData.nLineHeight * 0.5
  local nX, nY = oTextContainer.ParentWidget:GetLocation()
  local nTop = nY - nMargin * 0.5
  local nBottom = nY + oTextContainer.CustomData.nMaxHeight - nMargin - nMargin * 0.5
  for n, oLine in pairs(oTextContainer.CustomData.tLines) do
    nX, nY = oLine:GetLocation()
    if nTop > nY then
      oLine:SetTranslucency(math.max((1 - (nTop - nY) / nMargin) * 255, 0))
    elseif nBottom < nY then
      oLine:SetTranslucency(math.max((1 - (nY - nBottom) / nMargin) * 255, 0))
    else
      oLine:SetTranslucency(255)
    end
  end
end

function _ValidateParameter(Parameter, sType, DefaultValue)
  if type(Parameter) == sType then
    return Parameter
  else
    return DefaultValue
  end
end

oSystemDialogBoxFlash = nil

function OpenSystemDialogBox(sTitle, sMessage, sButton)
  if oSystemDialogBoxFlash == nil then
    local oWidget = MrxGuiBase.GetWidgetByNameAndOwner("PDA", Player.GetLocalPlayer())
    if oWidget then
      oWidget:Close()
    end
    oWidget = MrxGuiBase.GetWidgetByName("Pause Layout")
    if oWidget then
      oWidget:Close()
      Sys.RequestGameState("ingame")
    end
    oSystemDialogBoxFlash = MrxGuiBase.FlashWidget:new()
    oSystemDialogBoxFlash:SetOwner(Player.GetLocalPlayer())
    oSystemDialogBoxFlash:SetLocation(160, 120, 480, 360)
    MrxGuiBase.AddWidget(oSystemDialogBoxFlash)
    oSystemDialogBoxFlash:SetSwfFile("dialog_box", SystemDialogBoxLoadedCallBack, {
      oSystemDialogBoxFlash,
      sTitle,
      sMessage,
      sButton
    })
  end
end

function SystemDialogBoxLoadedCallBack(oFlash, sTitle, sMessage, sButton)
  if oFlash ~= nil then
    MrxGuiBase.GetControlFocus(oFlash, true)
    oFlash:CallActionScriptCallback("onlineMessage", {
      sTitle,
      sMessage,
      1,
      sButton
    })
    oFlash:SetFlashEventHandler("onlineMessageClose", CloseSystemDialogBox, {})
  end
end

function CloseSystemDialogBox()
  Event.Create(Event.TimerRelative, {0.01, true}, CloseSystemDialogBoxDelayed, {oSystemDialogBoxFlash})
  oSystemDialogBoxFlash = nil
end

function CloseSystemDialogBoxDelayed(oFlash)
  if oFlash ~= nil then
    MrxGuiBase.ReleaseControlFocus(oFlash)
    MrxGuiBase.RemoveWidget(oFlash)
    oFlash:SetSwfFile(nil)
    oFlash:delete()
  end
end
