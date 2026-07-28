import("MrxGuiBase")
_bTutorialsOn = true

function DisplayTutorialForObject(uPlayerGuid, sMessage, uGuid, fCallback, tCallbackData)
  if Gui.FindGuiLocation then
    local nX, nY, nX2, nY2 = Gui.FindGuiLocation(uPlayerGuid, uGuid)
    return DisplayTutorial(uPlayerGuid, sMessage, nX, nY, nX2, nY2, nil, nil, fCallback, tCallbackData)
  end
  return false
end

function DisplayTutorial(uPlayerGuid, sMessage, nX1, nY1, nX2, nY2, sHorizAnchor, sVertAnchor, fCallback, tCallbackData)
  if "string" ~= type(sMessage) then
    return false
  end
  if not _GetTutorialsEnabled() then
    if "function" == type(fCallback) then
      if "table" ~= type(tCallbackData) then
        tCallbackData = {}
      end
      fCallback(unpack(tCallbackData))
    end
    return true
  else
    if nX1 and nY1 and (not nX2 or not nY2) then
      local nDimension = 10
      nX2 = nX1 + nDimension
      nY2 = nY1 + nDimension
      nX1 = nX1 - nDimension
      nY1 = nY1 - nDimension
    end
    return _CreateTutorial(sMessage, nX1, nY1, nX2, nY2, sHorizAnchor, sVertAnchor, fCallback, tCallbackData, uPlayerGuid)
  end
end

function _CreateTutorial(sMessage, nPointX1, nPointY1, nPointX2, nPointY2, sHorizAnchor, sVertAnchor, fCallback, tCallbackData, uPlayerGuid)
  local nX1, nY1, nX2, nY2, nArrowDirection, bArrowVert
  local oText = MrxGuiBase.TextWidget:new()
  oText:SetFont("english_20")
  oText:SetScale(1)
  oText:SetOwner(uPlayerGuid)
  sMessage = sMessage .. "[n][n][confirm] [Generic.Continue][n][action] [Generic.DisableTutorials]"
  oText:SetText(sMessage)
  oText:Wrap()
  if "number" ~= type(nPointX1) or "number" ~= type(nPointY1) then
    local nBorder = 5
    local nWidth = 418 - nBorder * 2 - 48
    oText:SetLocation(0, 0, nWidth, 0)
    oText:SetText(sMessage)
    oText:Wrap()
    local nHeight = oText:GetHeight()
    oText:SetLocation(320 - nWidth / 2, 240 - nHeight / 2, 320 + nWidth / 2, 240 + nHeight / 2)
    nX1, nY1, nX2, nY2 = oText:GetLocation()
    sHorizAnchor = sHorizAnchor or "center"
    sVertAnchor = sVertAnchor or "center"
  else
    local nAreaHoriz = (math.max(nPointX1, 640 - nPointX2) - 48 - 32) * 408
    local nAreaVert = (math.max(nPointY1, 480 - nPointY2) - 36 - 32) * 544
    local nBorder = 5
    if nAreaHoriz >= nAreaVert then
      local nXLeft = nPointX1
      local nXRight = 640 - nPointX2
      local nWidth
      if nXLeft >= nXRight then
        nWidth = _OptimizeSize(oText, nXLeft - nBorder * 2 - 32 - 48, 408)
        nX2 = nPointX1 - 32 - nBorder
        nX1 = nX2 - nWidth
        nArrowDirection = 1
      else
        nWidth = _OptimizeSize(oText, nXRight - nBorder * 2 - 32 - 48, 408)
        nX1 = nPointX2 + 32 + nBorder
        nX2 = nX1 + nWidth
        nArrowDirection = -1
      end
      oText:SetLocation(nX1, 0, nX2, 0)
      local nHeight = oText:GetHeight()
      nCenterY = (nPointY1 + nPointY2) * 0.5
      nY1 = nCenterY - nHeight * 0.5 - nBorder
      nY2 = nCenterY + nHeight * 0.5 + nBorder
      if nY1 < 36 then
        nY2 = nY2 + (36 - nY1)
        nY1 = nY1 + (36 - nY1)
      elseif 444 < nY2 then
        nY1 = nY1 - (nY2 - 444)
        nY2 = nY2 - (nY2 - 444)
      end
      oText:SetLocation(nX1, nY1 + nBorder, nX2, nY2 - nBorder)
      nX1 = nX1 - nBorder
      nX2 = nX2 + nBorder
    else
      bArrowVert = true
      local nCenterY = (nPointY2 + nPointY1) / 2
      local nYTop = nPointY1
      local nYBottom = 480 - nPointY2
      local nWidth, nHeight
      if nYTop >= nYBottom then
        nWidth = _OptimizeSize(oText, 544, nYTop - nBorder * 2 - 32 - 36, true)
        oText:SetLocation(0, 0, nWidth, 0)
        nHeight = oText:GetHeight()
        nY2 = nPointY1 - 32 - nBorder
        nY1 = nY2 - nHeight - nBorder
        nArrowDirection = 1
      else
        nWidth = _OptimizeSize(oText, 544, nYBottom - nBorder * 2 - 32 - 36, true)
        oText:SetLocation(0, 0, nWidth, 0)
        nHeight = oText:GetHeight()
        nY1 = nPointY2 + 32 + nBorder
        nY2 = nY1 + nHeight + nBorder
        nArrowDirection = -1
      end
      nCenterX = (nPointX1 + nPointX2) * 0.5
      nX1 = nCenterX - nWidth * 0.5 - nBorder
      nX2 = nCenterX + nWidth * 0.5 + nBorder
      if nX1 < 48 then
        nX2 = nX2 + (48 - nX1)
        nX1 = nX1 + (48 - nX1)
      elseif 544 < nX2 and nPointX1 <= 544 then
        nX1 = nX1 - (nX2 - 544)
        nX2 = nX2 - (nX2 - 544)
      end
      oText:SetLocation(nX1 + nBorder, nY1, nX2 - nBorder, nY2)
      nY1 = nY1 - nBorder
      nY2 = nY2 + nBorder
    end
  end
  local oContainer = MrxGuiBase.Widget:new()
  if sHorizAnchor and sVertAnchor then
    oContainer:SetLocation(nX1, nY1, nX2, nY2)
    oContainer:SetAnchoring(sHorizAnchor, sVertAnchor)
  else
    oContainer:SetLocation(0, 0, 640, 480)
    oContainer:SetFullscreen(true)
  end
  local oBackground = MrxGuiBase.ImageWidget:new()
  oBackground:SetLocation(nX1, nY1, nX2, nY2)
  oBackground:SetColor(0, 0, 0, 192)
  oBackground:AddChild(oText)
  local oArrow
  if nArrowDirection then
    oArrow = MrxGuiBase.SpriteWidget:new()
    oArrow:SetTexture("temp_tutorial_arrow")
    oArrow:SetTextureSize(128, 64)
    oArrow:SetFrameSize(32, 64)
    oArrow:SetFrame(0)
    oArrow:PlayAnimation(0, 3, 0.25, true)
    oArrow:SetOwner(uPlayerGuid)
    oArrow:SetAnchoring(sHorizAnchor, sVertAnchor)
    oContainer:AddChild(oArrow)
    if bArrowVert then
      local nCenterX = (nPointX1 + nPointX2) * 0.5
      if nArrowDirection < 0 then
        oArrow:SetLocation(nCenterX - 16, nY1 - 48, nCenterX + 16, nY1 + 16)
        oArrow:SetRotation(90)
        if sHorizAnchor and sVertAnchor then
          oContainer:SetLocation(nX1, nY1 - 48, nX2, nY2)
        end
      else
        oArrow:SetLocation(nCenterX - 16, nY2 - 16, nCenterX + 16, nY2 + 48)
        oArrow:SetRotation(270)
        if sHorizAnchor and sVertAnchor then
          oContainer:SetLocation(nX1, nY1, nX2, nY2 + 48)
        end
      end
    else
      local nCenterY = (nPointY1 + nPointY2) * 0.5
      if nArrowDirection < 0 then
        oArrow:SetLocation(nX1 - 32, nCenterY - 32, nX1, nCenterY + 32)
        if sHorizAnchor and sVertAnchor then
          oContainer:SetLocation(nX1 - 32, nY1, nX2, nY2)
        end
      else
        oArrow:SetLocation(nX2, nCenterY - 32, nX2 + 32, nCenterY + 32)
        oArrow:SetRotation(180)
        if sHorizAnchor and sVertAnchor then
          oContainer:SetLocation(nX1, nY1, nX2 + 32, nY2)
        end
      end
    end
  end
  if oArrow then
    oArrow:AddChild(oBackground)
  else
    oContainer:AddChild(oBackground)
  end
  oContainer:SetOwner(uPlayerGuid)
  oBackground:SetOwner(uPlayerGuid)
  MrxGuiBase.AddWidgetWithChildren(oContainer)
  oContainer:SetEventHandler("ControllerInput", _HandleInput)
  oContainer.CustomData.fCallback = fCallback
  oContainer.CustomData.tCallbackData = tCallbackData
  MrxGuiBase.GetControlFocus(oContainer, true)
  return true
end

_knIncrement = 25

function _OptimizeSize(oText, nMaxWidth, nMaxHeight, bEnforceHeight)
  local nDifference = 10000
  local nNewDifference = 9999
  local nTestHeight = 0
  local nTestWidth = nMaxWidth
  oText:SetLocation(0, 0, nMaxWidth, 0)
  nTestWidth = nTestWidth - _knIncrement
  while nMaxHeight > nTestHeight and nDifference > nNewDifference and 0 < nTestWidth do
    oText:SetLocation(0, 0, nTestWidth, 0)
    nTestHeight = oText:GetHeight()
    nDifference = nNewDifference
    nNewDifference = math.abs(nTestWidth - nTestHeight)
    if nDifference > nNewDifference and nMaxHeight > nTestHeight and nTestWidth > nTestHeight then
      nTestWidth = nTestWidth - _knIncrement
    end
  end
  if bEnforceHeight then
    return nTestWidth + _knIncrement
  end
  return math.min(nTestWidth + _knIncrement, nMaxWidth)
end

function _HandleInput(oTutorial, tInput)
  local bExit
  if tInput.ButtonPress == MrxGuiBase.Joystick.BUTTON_PAD2_D then
    bExit = true
  elseif tInput.ButtonPress == MrxGuiBase.Joystick.BUTTON_PAD2_U then
    _SetTutorialsEnabled(false)
    bExit = true
  end
  if bExit then
    Sound.CueSound(0, "ui_HUD_Continue")
    if oTutorial.CustomData.fCallback then
      local tData = oTutorial.CustomData.tCallbackData or {}
      oTutorial.CustomData.fCallback(unpack(tData))
    end
    _DeleteTutorial(oTutorial)
  end
end

function _HandleStateChange(oTutorial, vStateInfo)
end

function _DeleteTutorial(oTutorial)
  MrxGuiBase.ReleaseControlFocus(oTutorial)
  MrxGuiBase.RemoveWidgetWithChildren(oTutorial)
  _DeleteChildren(oTutorial)
  oTutorial:delete()
end

function _DeleteChildren(oWidget)
  local tChildren = oWidget:GetChildren()
  for nIndex, oChild in pairs(tChildren) do
    _DeleteChildren(oChild)
    oWidget:RemoveChild(oChild)
    oChild:delete()
  end
end

function _GetTutorialsEnabled()
  if Sys.TutorialsEnabled then
    return Sys.TutorialsEnabled()
  end
  return _bTutorialsOn
end

function _SetTutorialsEnabled(bEnable)
  if Sys.SetTutorialsEnabled then
    Sys.SetTutorialsEnabled(bEnable)
  else
    _bTutorialsOn = bEnable
  end
end

function TutorialWidgetInitialize(oTutorial)
  local tChildren = oTutorial:GetChildren()
  oTutorial.CustomData.oInfo = tChildren[1]
  oTutorial.CustomData.oText = tChildren[2]
  InitInfoImage(tChildren[1])
  oTutorial.SetText = SetTutorialWidgetText
  oTutorial.PushToFront = PushTutorialToFront
end

function SetTutorialWidgetText(oTutorial, sText)
  local oInfo = oTutorial.CustomData.oInfo
  local oText = oTutorial.CustomData.oText
  if sText then
    oText:SetText(sText)
    oText:SetVisible(true)
    oText:PerformTextAnimation("typewriter")
    local nWidth = oText:GetWidth()
    oInfo:Resize(nWidth)
    oInfo:Show()
    local nX = oInfo:GetLocation()
    oText:SetLocation(nX)
    Sound.CueSound(0, "ui_signal_ding_up")
  else
    oText:SetVisible(false)
    oInfo:Hide()
  end
end

function InitInfoImage(oImage)
  oImage:SetTranslucency(0)
  local tChildren = oImage:GetChildren()
  local nX1, nY1, nX2, nY2 = oImage:GetLocation()
  oImage.CustomData.oInfo = tChildren[1]
  oImage.CustomData.oMid = tChildren[2]
  oImage.CustomData.oEnd = tChildren[3]
  oImage.CustomData.oStart = tChildren[4]
  oImage.CustomData.nMinWidth = nX2 - nX1
  oImage.Resize = ResizeInfoImage
  oImage.Show = ShowInfoImage
  oImage.Hide = HideInfoImage
  oImage.CustomData.sCurrentAnimation = "none"
  oImage.CustomData.nInfoStart = oImage.CustomData.oInfo:AddAnimationPoint({})
  oImage.CustomData.nInfoEnd = oImage.CustomData.oInfo:AddAnimationPoint({})
  oImage.CustomData.nMidStart = oImage.CustomData.oMid:AddAnimationPoint({})
  oImage.CustomData.nMidEnd = oImage.CustomData.oMid:AddAnimationPoint({})
  oImage.CustomData.nStartStart = oImage.CustomData.oStart:AddAnimationPoint({})
  oImage.CustomData.nStartEnd = oImage.CustomData.oStart:AddAnimationPoint({})
end

function ShowInfoImage(oImage)
  oImage.CustomData.bVisible = true
  if "open" == oImage.CustomData.sCurrentAnimation then
    return
  end
  oImage.CustomData.sCurrentAnimation = "open"
  oImage:SetTranslucency(255)
  local nX1, nY1, nX2, nY2 = oImage:GetLocation()
  local oInfo = oImage.CustomData.oInfo
  local oMid = oImage.CustomData.oMid
  local oEnd = oImage.CustomData.oEnd
  local oStart = oImage.CustomData.oStart
  local nEndX1, nEndY1, nEndX2 = oEnd:GetLocation()
  local nEndWidth = nEndX2 - nEndX1
  local nTime = 0.5
  oInfo:SetAnimationPoint(oImage.CustomData.nInfoStart, {
    y = nY2 - 2,
    y2 = nY2 - 2
  })
  oInfo:SetAnimationPoint(oImage.CustomData.nInfoEnd, {y = nY1, y2 = nY2})
  oInfo:AnimateToPoint(oImage.CustomData.nInfoStart, 0, true)
  oMid:SetAnimationPoint(oImage.CustomData.nMidStart, {
    x = nX2 - nEndWidth,
    x2 = nX2 - nEndWidth
  })
  oMid:SetAnimationPoint(oImage.CustomData.nMidEnd, {
    x = nX1 + nEndWidth,
    x2 = nX2 - nEndWidth
  })
  oMid:SetLocation(nX2 - nEndWidth, nil, nX2 - nEndWidth, nil)
  oMid:AnimateToPoint(oImage.CustomData.nMidEnd, nTime, true)
  oStart:SetAnimationPoint(oImage.CustomData.nStartStart, {
    x = nX2 - nEndWidth * 2,
    x2 = nX2 - nEndWidth
  })
  oStart:SetAnimationPoint(oImage.CustomData.nStartEnd, {
    x = nX1,
    x2 = nX1 + nEndWidth
  })
  oStart:SetLocation(nX2 - nEndWidth * 2, nX2 - nEndWidth)
  oStart:AnimateToPoint(oImage.CustomData.nStartEnd, nTime, true, _ExecAnim, {
    oInfo,
    oImage.CustomData.nInfoEnd,
    nTime * 0.5
  })
end

function HideInfoImage(oImage)
  if "close" == oImage.CustomData.sCurrentAnimation then
    return
  end
  oImage.CustomData.sCurrentAnimation = "close"
  local nX1, nY1, nX2, nY2 = oImage:GetLocation()
  local oInfo = oImage.CustomData.oInfo
  local oMid = oImage.CustomData.oMid
  local oEnd = oImage.CustomData.oEnd
  local oStart = oImage.CustomData.oStart
  local nEndX1, nEndY1, nEndX2 = oEnd:GetLocation()
  local nEndWidth = nEndX2 - nEndX1
  local nTime = 0.5
  oInfo:SetAnimationPoint(oImage.CustomData.nInfoStart, {
    y = nY2 - 2,
    y2 = nY2 - 2
  })
  oInfo:SetAnimationPoint(oImage.CustomData.nInfoEnd, {y = nY1, y2 = nY2})
  oInfo:SetLocation(nil, nY1, nil, nY2)
  oInfo:AnimateToPoint(oImage.CustomData.nInfoStart, nTime * 0.5, true, _ExecTwoAnims, {
    oMid,
    oImage.CustomData.nMidStart,
    nTime,
    oStart,
    oImage.CustomData.nStartStart,
    nTime,
    oImage
  })
  oMid:SetAnimationPoint(oImage.CustomData.nMidStart, {
    x = nX2 - nEndWidth,
    x2 = nX2 - nEndWidth
  })
  oMid:SetAnimationPoint(oImage.CustomData.nMidEnd, {
    x = nX1 + nEndWidth,
    x2 = nX2 - nEndWidth
  })
  oMid:AnimateToPoint(oImage.CustomData.nMidEnd, 0, true)
  oStart:SetAnimationPoint(oImage.CustomData.nStartStart, {
    x = nX2 - nEndWidth * 2,
    x2 = nX2 - nEndWidth
  })
  oStart:SetAnimationPoint(oImage.CustomData.nStartEnd, {
    x = nX1,
    x2 = nX1 + nEndWidth
  })
  oStart:AnimateToPoint(oImage.CustomData.nStartEnd, 0, true)
end

function _ExecAnim(unused, oWidget, nPoint, nTime)
  oWidget:AnimateToPoint(nPoint, nTime, true)
end

function _ExecTwoAnims(unused, oWidget1, nPoint1, nTime1, oWidget2, nPoint2, nTime2, oParent)
  oWidget1:AnimateToPoint(nPoint1, nTime1, true)
  oWidget2:AnimateToPoint(nPoint2, nTime2, true, _EndAnim, {oParent})
end

function _EndAnim(oUnused, oParent)
  oParent:SetTranslucency(0)
  oParent.CustomData.bVisible = false
end

function ResizeInfoImage(oImage, nWidth)
  local bAnimateResize = oImage.CustomData.bVisible
  nWidth = math.max(nWidth, oImage.CustomData.nMinWidth)
  local nX1, nY1, nX2, nY2 = oImage:GetLocation()
  local nEndX1, nEndY1, nEndX2 = oImage.CustomData.oEnd:GetLocation()
  local nEndWidth = nEndX2 - nEndX1
  local nInfoX, nInfoY = oImage.CustomData.oInfo:GetLocation()
  local nStartX = oImage.CustomData.oStart:GetLocation()
  local nMidX1, nMidY, nMidX2 = oImage.CustomData.oMid:GetLocation()
  oImage:SetLocation(nX2 - nWidth, nil, nX2, nil)
  oImage.CustomData.oInfo:SetLocation(nX2 - nWidth, nY1)
  if bAnimateResize then
    oImage.CustomData.oInfo:SetLocation(nInfoX, nInfoY)
    oImage.CustomData.oStart:SetLocation(nStartX)
    oImage.CustomData.oMid:SetLocation(nMidX1, nil, nMidX2)
    PerformResizeAnimation(oImage, nWidth)
  end
  oImage.CustomData.oEnd:SetLocation(nX2 - nEndWidth, nil, nX2)
end

function PerformResizeAnimation(oImage, nWidth)
  local nX1, nY1, nX2, nY2 = oImage:GetLocation()
  local oInfo = oImage.CustomData.oInfo
  local oMid = oImage.CustomData.oMid
  local oEnd = oImage.CustomData.oEnd
  local oStart = oImage.CustomData.oStart
  local nEndX1, nEndY1, nEndX2 = oEnd:GetLocation()
  local nEndWidth = nEndX2 - nEndX1
  local nTime = 0.5
  local nInfoX1, nInfoY1, nInfoX2 = oInfo:GetLocation()
  oInfo:SetAnimationPoint(oImage.CustomData.nInfoEnd, {
    x = nX1,
    y = nY1,
    x2 = nX1 + (nInfoX2 - nInfoX1),
    y2 = nY2
  })
  oInfo:AnimateToPoint(oImage.CustomData.nInfoEnd, nTime, true)
  oMid:SetAnimationPoint(oImage.CustomData.nMidEnd, {
    x = nX1 + nEndWidth,
    x2 = nX2 - nEndWidth
  })
  oMid:AnimateToPoint(oImage.CustomData.nMidEnd, nTime, true)
  oStart:SetAnimationPoint(oImage.CustomData.nStartEnd, {
    x = nX1,
    x2 = nX1 + nEndWidth
  })
  oStart:AnimateToPoint(oImage.CustomData.nStartEnd, nTime, true)
end

function PushTutorialToFront(oTutorial)
  local tChildren = oTutorial:GetChildren()
  MrxGuiBase.PushWidgetToFront(tChildren[2])
  local oImage = tChildren[1]
  tChildren = oImage:GetChildren()
  MrxGuiBase.PushWidgetToFront(tChildren[1])
  MrxGuiBase.PushWidgetToFront(tChildren[2])
  MrxGuiBase.PushWidgetToFront(tChildren[3])
  MrxGuiBase.PushWidgetToFront(tChildren[4])
end
