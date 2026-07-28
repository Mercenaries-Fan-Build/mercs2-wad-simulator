import("MrxGuiBase")
_ksFontSmall = "english_18"
_ksFont = "english_20"
_knScale = 1
_knScaleBig = 1
_knTextR = 156
_knTextG = 154
_knTextB = 133
_knTextLitR = 210
_knTextLitG = 210
_knTextLitB = 190
_ksAcceptSound = "ui_PDA_Accept"
_ksCancelSound = "ui_PDA_Cancel"
_ksChangeSound = "ui_PDA_Scroll"

function DisplayNumericBox(uPlayerGuid, sMessage, sPostFixMessage, sPrefix, sSuffix, nDefaultValue, nMinimumValue, nMaximumValue, nDefaultDigit, nMinimumDigit, nMaximumDigit, fAcceptCallback, tAcceptCallbackArgs, fCancelCallback, tCancelCallbackArgs, nXOffset, nYOffset, sHorizAnchor, sVertAnchor, bPause)
  if "userdata" ~= type(uPlayerGuid) then
    return
  end
  if "string" ~= type(sMessage) then
    return
  end
  nMinimumValue = _ValidateParameter(nMinimumValue, "number", nil)
  nMaximumValue = _ValidateParameter(nMaximumValue, "number", nil)
  nDefaultValue = _ValidateParameter(nDefaultValue, "number", nMinimumValue)
  nMinimumDigit = _ValidateParameter(nMinimumDigit, "number", 0)
  nMaximumDigit = _ValidateParameter(nMaximumDigit, "number", 9)
  nDefaultDigit = _ValidateParameter(nDefaultDigit, "number", nMinimumDigit)
  fAcceptCallback = _ValidateParameter(fAcceptCallback, "function", nil)
  tAcceptCallbackArgs = _ValidateParameter(tAcceptCallbackArgs, "table", {})
  fCancelCallback = _ValidateParameter(fCancelCallback, "function", nil)
  tCancelCallbackArgs = _ValidateParameter(tCancelCallbackArgs, "table", {})
  if nil == bPause then
    bPause = true
  end
  LTILibName.ChangeShellState(true)
  local oBox = _BuildNumericBox(sMessage, sPostFixMessage, sPrefix, sSuffix, nDefaultValue, nMinimumValue, nMaximumValue, nDefaultDigit, nMinimumDigit, nMaximumDigit, fAcceptCallback, tAcceptCallbackArgs, fCancelCallback, tCancelCallbackArgs, nXOffset, nYOffset, sHorizAnchor, sVertAnchor)
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

_knCursorHeight = 70

function _BuildNumericBox(sMessage, sPostFixMessage, sPrefix, sSuffix, nDefaultValue, nMinimumValue, nMaximumValue, nDefaultDigit, nMinimumDigit, nMaximumDigit, fAcceptCallback, tAcceptCallbackArgs, fCancelCallback, tCancelCallbackArgs, nXOffset, nYOffset, sHorizAnchor, sVertAnchor)
  local nStartX = 110
  local nStartY = 100
  local nWidth = 358
  local nBorder = 10
  local nYBorder = 20
  local nCurrentY = nStartY
  local oNumericBox = MrxGuiBase.ImageWidget:new()
  oNumericBox:SetLocation(nStartX, nStartY, nStartX + nWidth, 400)
  oNumericBox.BasicData.bContainer = true
  oNumericBox:SetOwner(uPlayerGuid)
  oNumericBox:SetVisible(false)
  oNumericBox.CustomData.oSelectableList = {}
  oNumericBox.CustomData.nHighlightedIdx = -1
  local oNumericBoxMessageText = MrxGuiBase.TextWidget:new()
  oNumericBoxMessageText:SetLocation(nStartX + nBorder, nCurrentY + nYBorder, nStartX + nWidth - nBorder, 390)
  oNumericBoxMessageText:SetFont(_ksFontSmall)
  oNumericBoxMessageText:SetScale(_knScale)
  oNumericBoxMessageText:SetColor(_knTextLitR, _knTextLitG, _knTextLitB)
  oNumericBoxMessageText:SetText(sMessage)
  oNumericBoxMessageText:Wrap()
  oNumericBoxMessageText:SetOwner(uPlayerGuid)
  oNumericBoxMessageText.ParentWidget = oNumericBox
  nCurrentY = nCurrentY + oNumericBoxMessageText:GetHeight() + nYBorder * 2.5
  local nCurrentX = nBorder + nStartX
  local oNumericBoxPrefixText = MrxGuiBase.TextWidget:new()
  oNumericBoxPrefixText:SetLocation(nCurrentX, nCurrentY, nStartX + nWidth - nBorder, 390)
  oNumericBoxPrefixText:SetFont(_ksFont)
  oNumericBoxPrefixText:SetScale(_knScaleBig)
  oNumericBoxPrefixText:SetColor(_knTextR, _knTextG, _knTextB)
  oNumericBoxPrefixText:SetText(sPrefix)
  oNumericBoxPrefixText:SetOwner(uPlayerGuid)
  oNumericBoxPrefixText.ParentWidget = oNumericBox
  nCurrentX = nCurrentX + oNumericBoxPrefixText:GetWidth()
  local oNumericBoxDigits = MrxGuiBase.ImageWidget:new()
  oNumericBoxDigits:SetTranslucency(0)
  oNumericBoxDigits:SetLocation(nCurrentX, nCurrentY, nStartX + nWidth - nBorder, 390)
  oNumericBoxDigits:SetOwner(uPlayerGuid)
  oNumericBoxDigits.ParentWidget = oNumericBox
  local nDigitHeight = 0
  local nDigitWidth = 10
  local tDigitBgs = {}
  for nIndex = nMaximumDigit, nMinimumDigit, -1 do
    local oDigitText = MrxGuiBase.TextWidget:new()
    oDigitText:SetLocation(nCurrentX, nCurrentY, nStartX + nWidth - nBorder, nCurrentY)
    oDigitText:SetFont(_ksFont)
    oDigitText:SetScale(_knScaleBig)
    oDigitText:SetColor(_knTextR, _knTextG, _knTextB)
    oDigitText:SetText("0")
    oDigitText:SetJustification("center")
    oDigitText:SetOwner(uPlayerGuid)
    oDigitText.ParentWidget = oNumericBoxDigits
    oNumericBoxDigits:AddChild(oDigitText)
    oDigitText.CustomData.nValue = 0
    oDigitText.CustomData.nScale = math.pow(10, nIndex)
    local nWidth = oDigitText:GetWidth() * 1.5
    nDigitWidth = nWidth
    local nHeight = oDigitText:GetHeight()
    oDigitText:SetLocation(nCurrentX, nCurrentY, nCurrentX + nWidth, nCurrentY + nHeight)
    local oDigitBackground = MrxGuiBase.ImageWidget:new()
    oDigitBackground:SetColor(63, 59.25, 42.75)
    oDigitBackground:SetTranslucency(255)
    oDigitBackground:SetLocation(nCurrentX, nCurrentY, nCurrentX + nWidth, nCurrentY + nHeight)
    oDigitBackground:SetOwner(uPlayerGuid)
    oDigitBackground.ParentWidget = oNumericBox
    oDigitBackground:SetHighlightable(1)
    table.insert(oNumericBox.CustomData.oSelectableList, oDigitBackground)
    Debug.Printf("building digits:::" .. tostring(nIndex) .. "id:::" .. tostring(oDigitBackground.BasicData.uId))
    table.insert(tDigitBgs, oDigitBackground)
    nCurrentX = nCurrentX + nWidth + 2
    nDigitHeight = math.max(nDigitHeight, nHeight)
  end
  local oCursor = MrxGuiBase.ImageWidget:new()
  oCursor:SetColor(84, 79, 57)
  oCursor:SetTranslucency(0)
  oCursor:SetVisible(false)
  oCursor:SetLocation(nStartX + nBorder, 0, nStartX + nBorder + nDigitWidth, _knCursorHeight)
  oCursor:SetOwner(uPlayerGuid)
  oCursor.ParentWidget = oNumericBox
  oCursor.CustomData.nClosePoint = oCursor:AddAnimationPoint({x = 1, x2 = 1})
  oCursor.CustomData.nOpenPoint = oCursor:AddAnimationPoint({x = 1, x2 = 1})
  oCursor.CustomData.nPulseHighPoint = oCursor:AddAnimationPoint({TranslucencyLevel = 255})
  oCursor.CustomData.nPulseLowPoint = oCursor:AddAnimationPoint({TranslucencyLevel = 100})
  oCursor:SetIgnoresPause(true)
  local oCursorBg = MrxGuiBase.ImageWidget:new()
  oCursorBg:SetColor(84, 79, 57)
  oCursorBg:SetTranslucency(205)
  oCursorBg:SetLocation(nStartX + nBorder, 0, nStartX + nBorder + nDigitWidth, _knCursorHeight)
  oCursorBg:SetOwner(uPlayerGuid)
  oCursorBg.ParentWidget = oCursor
  oCursorBg.CustomData.nPulseHighPoint = oCursorBg:AddAnimationPoint({TranslucencyLevel = 255})
  oCursorBg.CustomData.nPulseLowPoint = oCursorBg:AddAnimationPoint({TranslucencyLevel = 100})
  oCursorBg:SetIgnoresPause(true)
  oCursor:AddChild(oCursorBg)
  Pulse(oCursorBg)
  local oUpCallout = MrxGuiBase.ImageWidget:new()
  oUpCallout:SetTexture("global_gui_hud02")
  oUpCallout:SetTextureCoordinates(0.001953, 0.947266, 0.162109, 0.986328)
  oUpCallout:SetLocation(nStartX + nBorder, 0, nStartX + nBorder + nDigitWidth, nDigitWidth)
  oUpCallout:SetColor(_knTextR, _knTextR, _knTextR)
  oUpCallout:SetHighlightable(1)
  table.insert(oNumericBox.CustomData.oSelectableList, oUpCallout)
  oCursor:AddChild(oUpCallout)
  local oDownCallout = MrxGuiBase.ImageWidget:new()
  oDownCallout:SetTexture("global_gui_hud02")
  oDownCallout:SetTextureCoordinates(0.001953, 0.986328, 0.162109, 0.947266)
  local nCalloutHeight = nDigitWidth
  oDownCallout:SetLocation(nStartX + nBorder, _knCursorHeight - nCalloutHeight, nStartX + nBorder + nDigitWidth, _knCursorHeight)
  oUpCallout:SetColor(_knTextR, _knTextR, _knTextR)
  oDownCallout:SetHighlightable(1)
  table.insert(oNumericBox.CustomData.oSelectableList, oDownCallout)
  oCursor:AddChild(oDownCallout)
  local oNumericBoxSuffixText = MrxGuiBase.TextWidget:new()
  oNumericBoxSuffixText:SetLocation(nCurrentX, nCurrentY, nStartX + nWidth - nBorder, 390)
  oNumericBoxSuffixText:SetFont(_ksFont)
  oNumericBoxSuffixText:SetScale(_knScaleBig)
  oNumericBoxSuffixText:SetColor(_knTextR, _knTextG, _knTextB)
  oNumericBoxSuffixText:SetText(sSuffix)
  oNumericBoxSuffixText:SetOwner(uPlayerGuid)
  oNumericBoxSuffixText.ParentWidget = oNumericBox
  nCurrentY = nCurrentY + nDigitHeight + (_knCursorHeight - nDigitHeight) * 0.5 + 8
  oNumericBox.CustomData.nSelectedIndex = nMaximumDigit - nDefaultDigit + 1
  if not oNumericBoxDigits:GetChildren()[oNumericBox.CustomData.nSelectedIndex] then
    oNumericBox.CustomData.nSelectedIndex = 1
  end
  local oSelectedText = oNumericBoxDigits:GetChildren()[oNumericBox.CustomData.nSelectedIndex]
  local nX1, nY1, nX2, nY2 = oSelectedText:GetLocation()
  local nY = (nY2 + nY1) * 0.5 - _knCursorHeight * 0.5
  oCursor:SetLocation(nX1, nY, nX2, nY + _knCursorHeight)
  oSelectedText:SetColor(_knTextLitR, _knTextLitG, _knTextLitB)
  local oPostFixMessage
  if sPostFixMessage then
    oPostFixMessage = MrxGuiBase.TextWidget:new()
    oPostFixMessage:SetFont(_ksFontSmall)
    oPostFixMessage:SetJustification("left")
    oPostFixMessage:SetColor(_knTextR, _knTextG, _knTextB)
    oPostFixMessage:SetText(sPostFixMessage)
    oPostFixMessage:SetOwner(uPlayerGuid)
    local nMessageHeight = oPostFixMessage:GetHeight()
    oPostFixMessage:SetLocation(nStartX + nBorder, nCurrentY, nStartX + nWidth - nBorder, nCurrentY + nCalloutHeight)
    nCurrentY = nCurrentY + nMessageHeight + nBorder
  end
  local oCallouts = MrxGuiBase.TextWidget:new()
  oCallouts:SetFont(_ksFontSmall)
  oCallouts:SetJustification("center")
  oCallouts:SetColor(_knTextR, _knTextG, _knTextB)
  oCallouts:SetText("[move] [PDA.Common.MoveSelection]  [confirm] [Generic.Confirm]")
  oCallouts:SetOwner(uPlayerGuid)
  local oCalloutHeight = oCallouts:GetHeight()
  oCallouts:SetLocation(nStartX + nBorder, nCurrentY, nStartX + nWidth - nBorder, nCurrentY + nCalloutHeight)
  nCurrentY = nCurrentY + nCalloutHeight - 10
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
  local nPieceWidth = 542 * nScaleFactor
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
    oNumericBox:AddChild(tBg[n])
    n = n + 1
  end
  n = 1
  while tDigitBgs[n] do
    oNumericBox:AddChild(tDigitBgs[n])
    n = n + 1
  end
  oNumericBox:AddChild(oNumericBoxMessageText)
  oNumericBox:AddChild(oCursor)
  oNumericBox:AddChild(oNumericBoxPrefixText)
  oNumericBox:AddChild(oNumericBoxDigits)
  oNumericBox:AddChild(oNumericBoxSuffixText)
  oNumericBox:AddChild(oCallouts)
  if oPostFixMessage then
    oNumericBox:AddChild(oPostFixMessage)
  end
  oNumericBox.CustomData.oCursor = oCursor
  oNumericBox.CustomData.oDigits = oNumericBoxDigits
  oNumericBox.CustomData.nMinimumDigit = nMinimumDigit
  oNumericBox.CustomData.nMaximumDigit = nMaximumDigit
  oNumericBox.CustomData.nMinimumValue = nMinimumValue
  oNumericBox.CustomData.nMaximumValue = nMaximumValue
  oNumericBox.CustomData.fAcceptCallback = fAcceptCallback
  oNumericBox.CustomData.tAcceptCallbackArgs = tAcceptCallbackArgs
  oNumericBox.CustomData.fCancelCallback = fCancelCallback
  oNumericBox.CustomData.tCancelCallbackArgs = tCancelCallbackArgs
  local nRealHeight = nCurrentY - nStartY
  nStartY = 240 - nRealHeight / 2
  if nStartY < 0 then
    nStartY = 0
  end
  oNumericBox:SetLocation(nStartX, nStartY)
  oNumericBox:SetCoordinates(nStartX, nStartY, nStartX + nWidth, nStartY + nRealHeight)
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
    oNumericBox:SetAnchoring(sHorizAnchor, sVertAnchor)
    oNumericBox:SetLocation(nNewX, nNewY)
    local oOptionText = MrxGuiBase.TextWidget:new()
    oOptionText:SetLocation(nNewX + nWidth - 88.666 - 10, nNewY + 30, nNewX + nWidth - 10, nNewY + 70)
    oOptionText:SetFont("english_18")
    oOptionText:SetJustification("center")
    oOptionText:SetColor(_knTextR, _knTextG, _knTextB)
    oOptionText:SetText("[Generic.Accept]")
    oOptionText:SetHighlightable(1)
    oOptionText:SetOwner(uPlayerGuid)
    oOptionText.ParentWidget = oNumericBox
    oOptionText.CustomData.nHeight = oOptionText:GetHeight()
    _BuildStrokes(oOptionText, nNewX + nWidth - 88.666 - 10, nNewY + 20, nNewX + nWidth - 10, nNewY + 70)
    local oAcceptBox = MrxGuiBase.ImageWidget:new()
    oAcceptBox:SetLocation(nNewX + nWidth - 88.666 - 10, nNewY + 20, nNewX + nWidth - 10, nNewY + 70)
    oAcceptBox:SetColor(_knTextR, _knTextG, _knTextB, 0)
    oNumericBox:AddChild(oOptionText)
    oNumericBox:AddChild(oAcceptBox)
    oAcceptBox:SetHighlightable(1)
    table.insert(oNumericBox.CustomData.oSelectableList, oAcceptBox)
    local oOptionText2 = MrxGuiBase.TextWidget:new()
    oOptionText2:SetLocation(nNewX + nWidth - 88.666 - 10, nNewY + nHeight - 110, nNewX + nWidth - 10, nNewY + nHeight - 70)
    oOptionText2:SetFont("english_18")
    oOptionText2:SetJustification("center")
    oOptionText2:SetColor(_knTextR, _knTextG, _knTextB)
    oOptionText2:SetText("[Generic.Cancel]")
    oOptionText2:SetHighlightable(1)
    oOptionText2:SetOwner(uPlayerGuid)
    oOptionText2.ParentWidget = oNumericBox
    oOptionText2.CustomData.nHeight = oOptionText2:GetHeight()
    _BuildStrokes(oOptionText2, nNewX + nWidth - 88.666 - 10, nNewY + nHeight - 120, nNewX + nWidth - 10, nNewY + nHeight - 70)
    local oCancelBox = MrxGuiBase.ImageWidget:new()
    oCancelBox:SetLocation(nNewX + nWidth - 88.666 - 10, nNewY + nHeight - 120, nNewX + nWidth - 10, nNewY + nHeight - 70)
    oCancelBox:SetColor(_knTextR, _knTextG, _knTextB, 0)
    oNumericBox:AddChild(oOptionText2)
    oNumericBox:AddChild(oCancelBox)
    oCancelBox:SetHighlightable(1)
    table.insert(oNumericBox.CustomData.oSelectableList, oCancelBox)
  end
  MrxGuiBase.AddWidgetWithChildren(oNumericBox)
  oNumericBox:SetEventHandler("OnMouseMove", _HandleScrollUpdate)
  oNumericBox:SetEventHandler("ControllerInput", _HandleInputEvent)
  oNumericBox._ComputeValue = _ComputeValue
  oNumericBox._SetValue = _SetValue
  oNumericBox._ChangeSelection = _ChangeSelection
  oNumericBox._ModifySelection = _ModifySelection
  oNumericBox:_SetValue(nDefaultValue)
  return oNumericBox
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

function _ComputeValue(oNumericBox)
  local nTotalValue = 0
  for _, oDigit in ipairs(oNumericBox.CustomData.oDigits:GetChildren()) do
    nTotalValue = nTotalValue + oDigit.CustomData.nValue * oDigit.CustomData.nScale
  end
  return nTotalValue
end

function _SetValue(oNumericBox, nTotalValue)
  local sTotalText = string.format("%0" .. oNumericBox.CustomData.nMaximumDigit + 1 .. "d", nTotalValue)
  for nOffset, oDigit in ipairs(oNumericBox.CustomData.oDigits:GetChildren()) do
    local sDigitText = string.sub(sTotalText, nOffset, nOffset)
    oDigit.CustomData.nValue = tonumber(sDigitText)
    oDigit:SetText(sDigitText)
  end
end

function _ModifySelection(oNumericBox, nIncrement)
  local tChildren = oNumericBox.CustomData.oSelectableList
  local tChildren2 = oNumericBox.CustomData.oDigits:GetChildren()
  local oCurDigit = tChildren2[oNumericBox.CustomData.nSelectedIndex]
  if oNumericBox.CustomData.nSelectedIndex == #oNumericBox.CustomData.oSelectableList - 3 or oNumericBox.CustomData.nSelectedIndex == #oNumericBox.CustomData.oSelectableList - 2 then
    return
  end
  if oNumericBox.CustomData.nSelectedIndex == #oNumericBox.CustomData.oSelectableList - 1 then
    oNumericBox.CustomData.nSelectedIndex = #oNumericBox.CustomData.oSelectableList
    tChildren[#tChildren]:SetColor(_knTextLitR, _knTextLitG, _knTextLitB, 30)
    tChildren[#tChildren - 1]:SetColor(0, 0, 0, 0)
    return
  elseif oNumericBox.CustomData.nSelectedIndex == #oNumericBox.CustomData.oSelectableList then
    oNumericBox.CustomData.nSelectedIndex = #oNumericBox.CustomData.oSelectableList - 1
    tChildren[#tChildren]:SetColor(0, 0, 0, 0)
    tChildren[#tChildren - 1]:SetColor(_knTextLitR, _knTextLitG, _knTextLitB, 30)
    return
  end
  local nPrevValue = oNumericBox:_ComputeValue()
  local nNewValue = nPrevValue + nIncrement * oCurDigit.CustomData.nScale
  nNewValue = math.min(oNumericBox.CustomData.nMaximumValue, nNewValue)
  nNewValue = math.max(oNumericBox.CustomData.nMinimumValue, nNewValue)
  oNumericBox:_SetValue(nNewValue)
  oCurDigit:SetText(tostring(oCurDigit.CustomData.nValue))
end

function _ChangeSelection(oNumericBox, nIncrement)
  local tChildren = oNumericBox.CustomData.oSelectableList
  oNumericBox.CustomData.nSelectedIndex = oNumericBox.CustomData.nSelectedIndex + nIncrement
  if oNumericBox.CustomData.nSelectedIndex == #tChildren - 2 or oNumericBox.CustomData.nSelectedIndex == #tChildren - 3 then
    if 0 < nIncrement then
      oNumericBox.CustomData.nSelectedIndex = #tChildren - 1
      oNumericBox.CustomData.oSelectableList[#tChildren - 1]:SetColor(_knTextLitR, _knTextLitG, _knTextLitB, 30)
    else
      oNumericBox.CustomData.nSelectedIndex = #tChildren - 4
    end
  elseif oNumericBox.CustomData.nSelectedIndex == #tChildren - 1 then
    if nIncrement < 0 then
      oNumericBox.CustomData.nSelectedIndex = #tChildren - 4
    end
  elseif oNumericBox.CustomData.nSelectedIndex == #tChildren and 0 < nIncrement then
    oNumericBox.CustomData.nSelectedIndex = 1
  end
  if nIncrement < 0 then
    if oNumericBox.CustomData.nSelectedIndex < 1 then
      oNumericBox.CustomData.nSelectedIndex = #tChildren - 1
      oNumericBox.CustomData.nHighlightedIdx = #tChildren - 1
    end
  elseif not tChildren[oNumericBox.CustomData.nSelectedIndex] then
    oNumericBox.CustomData.nSelectedIndex = 1
  end
  if oNumericBox.CustomData.nSelectedIndex == #tChildren - 1 then
    tChildren[#tChildren]:SetColor(0, 0, 0, 0)
    tChildren[#tChildren - 1]:SetColor(_knTextLitR, _knTextLitG, _knTextLitB, 30)
  elseif oNumericBox.CustomData.nSelectedIndex == #tChildren then
    tChildren[#tChildren - 1]:SetColor(0, 0, 0, 0)
    tChildren[#tChildren]:SetColor(_knTextLitR, _knTextLitG, _knTextLitB, 30)
  else
    tChildren[#tChildren]:SetColor(0, 0, 0, 0)
    tChildren[#tChildren - 1]:SetColor(0, 0, 0, 0)
  end
  if oNumericBox.CustomData.nSelectedIndex >= #tChildren - 1 then
    oNumericBox.CustomData.oCursor:SetVisible(false)
    return
  else
    oNumericBox.CustomData.oCursor:SetVisible(true)
  end
  local oCurrentDigit = tChildren[oNumericBox.CustomData.nSelectedIndex]
  local nDestX1, nDestY1, nDestX2, nDestY2 = oCurrentDigit:GetLocation()
  local oCursor = oNumericBox.CustomData.oCursor
  local nCurX1, nCurY1, nCurX2, nCurY2 = oCursor:GetLocation()
  local nCloseY = (nCurY1 + nCurY2) * 0.5
  local nDestY = (nDestY2 + nDestY1) / 2 - _knCursorHeight * 0.5
  oCursor:SetAnimationPoint(oCursor.CustomData.nOpenPoint, {
    x = nDestX1,
    y = nDestY,
    x2 = nDestX2,
    y2 = nDestY + _knCursorHeight
  })
  oCursor:AnimateToPoint(oCursor.CustomData.nOpenPoint, 0.1, true)
  oCurrentDigit:SetColor(_knTextLitR, _knTextLitG, _knTextLitB)
end

function _CompleteAnimation(oCursor, nX1, nY1, nX2, nY2, oCurrentDigit)
  local nMidY = (nY1 + nY2) * 0.5
  oCursor:SetLocation(nX1, nMidY, nX2, nMidY)
  oCursor:SetAnimationPoint(oCursor.CustomData.nOpenPoint, {
    x = nX1,
    y = nY1,
    x2 = nX2,
    y2 = nY2
  })
  oCursor:AnimateToPoint(oCursor.CustomData.nOpenPoint, 0.075, true)
  oCurrentDigit:SetColor(_knTextLitR, _knTextLitG, _knTextLitB)
end

oldIdx = -1

function _HandleScrollUpdate(oBox, nDeltaTime)
  local nId = _GuiInternal.GetWidgetHighlightId()
  local downId = _GuiInternal.GetWidgetDownId()
  if nId == 0 then
    return
  end
  local bFound = false
  local count = #oBox.CustomData.oSelectableList
  for i, oItem in pairs(oBox.CustomData.oSelectableList) do
    if nId == oItem.BasicData.uId then
      _UnselectAll(oBox)
      bFound = true
      if i == count or i == count - 1 then
        oItem:SetColor(_knTextLitR, _knTextLitG, _knTextLitB, 30)
      else
        oItem:SetColor(_knTextLitR, _knTextLitR, _knTextLitR, 175)
      end
      oBox.CustomData.nHighlightedIdx = i
      break
    end
  end
  if bFound == false and oBox.CustomData.nHighlightedIdx > -1 then
    if oBox.CustomData.nSelectedIndex == oBox.CustomData.nHighlightedIdx and oBox.CustomData.nHighlightedIdx >= #oBox.CustomData.oSelectableList - 1 then
      return
    end
    oItem = oBox.CustomData.oSelectableList[oBox.CustomData.nHighlightedIdx]
    if oBox.CustomData.nHighlightedIdx == #oBox.CustomData.oSelectableList or oBox.CustomData.nHighlightedIdx == #oBox.CustomData.oSelectableList - 1 then
      oItem:SetColor(0, 0, 0, 0)
    else
      oItem:SetColor(63, 59.25, 42.75, 175)
    end
  end
end

function _UnselectAll(oBox)
  for i, oItem in pairs(oBox.CustomData.oSelectableList) do
    if i == #oBox.CustomData.oSelectableList or i == #oBox.CustomData.oSelectableList - 1 then
      oItem:SetColor(0, 0, 0, 0)
    else
      oItem:SetColor(63, 59.25, 42.75, 175)
    end
  end
end

function _HandleInputEvent(oNumericBox, tEvent)
  local count = #oNumericBox.CustomData.oSelectableList
  if MrxGuiBase.Joystick.BUTTON_PAD1_D == tEvent.ButtonPress or MrxGuiBase.Joystick.BUTTON_L_STICK_D == tEvent.ButtonPress then
    oNumericBox:_ModifySelection(-1)
    Sound.CueSound(0, _ksChangeSound)
  elseif MrxGuiBase.Joystick.BUTTON_PAD1_U == tEvent.ButtonPress or MrxGuiBase.Joystick.BUTTON_L_STICK_U == tEvent.ButtonPress then
    oNumericBox:_ModifySelection(1)
    Sound.CueSound(0, _ksChangeSound)
  elseif MrxGuiBase.Joystick.BUTTON_PAD1_L == tEvent.ButtonPress or MrxGuiBase.Joystick.BUTTON_L_STICK_L == tEvent.ButtonPress then
    _UnselectAll(oNumericBox)
    oNumericBox:_ChangeSelection(-1)
    Sound.CueSound(0, _ksChangeSound)
  elseif MrxGuiBase.Joystick.BUTTON_PAD1_R == tEvent.ButtonPress or MrxGuiBase.Joystick.BUTTON_L_STICK_R == tEvent.ButtonPress then
    _UnselectAll(oNumericBox)
    oNumericBox:_ChangeSelection(1)
    Sound.CueSound(0, _ksChangeSound)
  elseif MrxGuiBase.Joystick.BUTTON_PAD2_D == tEvent.ButtonPress then
    local tChildren = oNumericBox.CustomData.oSelectableList
    local nId = _GuiInternal.GetWidgetHighlightId()
    local newIndex = 0
    local bFound = false
    local bDoAccept = false
    local bDoCancel = false
    for nIdx, oItem in pairs(tChildren) do
      if nId == oItem.BasicData.uId then
        newIndex = nIdx
        bFound = true
      end
    end
    if bFound then
      if newIndex <= #tChildren - 4 then
        oNumericBox:_ChangeSelection(newIndex - oNumericBox.CustomData.nSelectedIndex)
        oNumericBox.CustomData.nSelectedIndex = newIndex
      elseif newIndex == #tChildren - 3 then
        oNumericBox:_ModifySelection(1)
      elseif newIndex == #tChildren - 2 then
        oNumericBox:_ModifySelection(-1)
      end
      if newIndex == #tChildren - 1 then
        bDoAccept = true
      elseif newIndex == #tChildren then
        bDoCancel = true
      end
    elseif oNumericBox.CustomData.nSelectedIndex == #tChildren - 1 then
      bDoAccept = true
    elseif oNumericBox.CustomData.nSelectedIndex == #tChildren then
      bDoCancel = true
    end
    if bDoAccept then
      MrxGuiBase.ReleaseControlFocus(oNumericBox)
      if oNumericBox.CustomData.fAcceptCallback then
        Sound.CueSound(0, _ksAcceptSound)
        table.insert(oNumericBox.CustomData.tAcceptCallbackArgs, oNumericBox:_ComputeValue())
        oNumericBox.CustomData.fAcceptCallback(unpack(oNumericBox.CustomData.tAcceptCallbackArgs))
      end
      oNumericBox:Close()
    elseif bDoCancel then
      MrxGuiBase.ReleaseControlFocus(oNumericBox)
      if oNumericBox.CustomData.fCancelCallback then
        Sound.CueSound(0, _ksCancelSound)
        table.insert(oNumericBox.CustomData.tAcceptCallbackArgs, oNumericBox:_ComputeValue())
        oNumericBox.CustomData.fCancelCallback(unpack(oNumericBox.CustomData.tCancelCallbackArgs))
      end
      oNumericBox:Close()
    end
    bFound = false
  elseif MrxGuiBase.Joystick.BUTTON_PAD2_R == tEvent.ButtonPress then
    MrxGuiBase.ReleaseControlFocus(oNumericBox)
    if oNumericBox.CustomData.fCancelCallback then
      Sound.CueSound(0, _ksCancelSound)
      table.insert(oNumericBox.CustomData.tAcceptCallbackArgs, oNumericBox:_ComputeValue())
      oNumericBox.CustomData.fCancelCallback(unpack(oNumericBox.CustomData.tCancelCallbackArgs))
    end
    oNumericBox:Close()
  end
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

function _ValidateParameter(Parameter, sType, DefaultValue)
  if type(Parameter) == sType then
    return Parameter
  else
    return DefaultValue
  end
end
