import("MrxGui")

function _CallWithOptionalArgs(fFunction, tArgs)
  if type(fFunction) == "function" then
    if type(tArgs) == "table" then
      return fFunction(unpack(tArgs))
    else
      return fFunction()
    end
  end
end

_knMaxOptionsPerPage = 8
_DialogBox = nil

function Init()
  _tOptions = {}
  _tOptionsOnEveryPage = {}
  _tOptionsToCallbacks = {}
  _tPageOptions = {}
  _nPages = 0
  _sCancelButtonOptionName = ""
end

function Reset()
  Init()
end

function Close()
  if _DialogBox then
    LTILibName.ChangeShellState(false)
    MrxGui.CloseDialogBox(_DialogBox)
    _DialogBox = nil
  end
end

function BindOptionToCancelButton(sOptionName)
  if _sCancelButtonOptionName == "" then
    _sCancelButtonOptionName = sOptionName
  end
end

function AddOption(sOptionName, fCallback, tCallbackArgs, bEveryPage, bBindToCancelButton)
  local tDest = _tOptions
  if bEveryPage then
    tDest = _tOptionsOnEveryPage
  end
  if bBindToCancelButton then
    BindOptionToCancelButton(sOptionName)
  end
  table.insert(tDest, sOptionName)
  _tOptionsToCallbacks[sOptionName] = {fCallback, tCallbackArgs}
end

function Display(sQuery)
  _nPages = 0
  local nTotalOptions = table.getn(_tOptions)
  local nOptionsOnEveryPage = table.getn(_tOptionsOnEveryPage)
  local nFreeOptions = _knMaxOptionsPerPage - nOptionsOnEveryPage
  while nTotalOptions > _nPages * nFreeOptions do
    _nPages = _nPages + 1
  end
  _Display(1, sQuery)
end

function _Display(nPage, sQuery)
  _tPageOptions = {}
  local nIndexForCancelButton
  if nPage < _nPages then
    table.insert(_tPageOptions, "Next page")
  end
  if 1 < nPage then
    table.insert(_tPageOptions, "Previous page")
  end
  local nOptionsOnEveryPage = table.getn(_tOptionsOnEveryPage)
  local nFreeOptions = _knMaxOptionsPerPage - nOptionsOnEveryPage
  local nPageStartIndex = (nPage - 1) * nFreeOptions + 1
  for i = nPageStartIndex, nPageStartIndex + nFreeOptions - 1 do
    local sOptionName = _tOptions[i]
    if sOptionName then
      table.insert(_tPageOptions, sOptionName)
      if _sCancelButtonOptionName == sOptionName then
        nIndexForCancelButton = table.getn(_tPageOptions)
      end
    else
      break
    end
  end
  for _, sOptionName in ipairs(_tOptionsOnEveryPage) do
    table.insert(_tPageOptions, sOptionName)
    if _sCancelButtonOptionName == sOptionName then
      nIndexForCancelButton = table.getn(_tPageOptions)
    end
  end
  local sDisplay = sQuery
  if _nPages > 1 then
    sDisplay = sDisplay .. " (Page " .. nPage .. "/" .. _nPages .. ")"
  end
  LTILibName.ChangeShellState(true)
  _DialogBox = MrxGui.DisplayDialogBox(Player.GetLocalPlayer(), sDisplay, _tPageOptions, nDefaultOption or 1, _ChooseOption, {nPage, sQuery}, nil, nil, nil, nil, nil, nIndexForCancelButton)
end

function _ChooseOption(nPage, sQuery, nSelectedIndex)
  local sOptionName = _tPageOptions[nSelectedIndex]
  if sOptionName == "Previous page" then
    _Display(nPage - 1, sQuery)
  elseif sOptionName == "Next page" then
    _Display(nPage + 1, sQuery)
  else
    _DialogBox = nil
    local tCallbackData = _tOptionsToCallbacks[sOptionName]
    LTILibName.ChangeShellState(false)
    if tCallbackData then
      _CallWithOptionalArgs(tCallbackData[1], tCallbackData[2])
    end
  end
end
