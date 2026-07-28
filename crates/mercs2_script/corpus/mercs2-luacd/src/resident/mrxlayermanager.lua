import("MrxUtil")
_tRequests = {}
_tOpQueue = {}
_tLoadedLayers = {}
_tLayersToBeAdded = {}
_knRequestTypeAdd = 1
_knRequestTypeRemove = 2
_knLayerStatusUnloaded = 1
_knLayerStatusPending = 2
_knLayerStatusLoaded = 3
_nLayersBeingProcessed = 0
local _knLayersToProcessCap = 10

function Init()
  _knOrigAssetRequestMax = Sys.GetAssetRequestMax()
end

function Add(vLayers, fCallback, tCallbackArgs, bCullDupes, bStatic, bClientNeedsLoadingScreen)
  _AddRequest(_knRequestTypeAdd, vLayers, fCallback, tCallbackArgs, bCullDupes, bStatic, bClientNeedsLoadingScreen)
end

function Remove(vLayers, fCallback, tCallbackArgs, bClientNeedsLoadingScreen)
  _AddRequest(_knRequestTypeRemove, vLayers, fCallback, tCallbackArgs, bClientNeedsLoadingScreen)
end

function RemoveDynamicLayers(fCallback, tCallbackArgs)
  local tLayers = {}
  for sLayerName, tLayerData in pairs(_tLoadedLayers) do
    table.insert(tLayers, sLayerName)
  end
  Remove(tLayers, fCallback, tCallbackArgs)
end

function RemoveAllLayers(tStaticLayers, fCallback, tCallbackArgs)
  local tLayers = tStaticLayers
  for sLayerName, tLayerData in pairs(_tLoadedLayers) do
    table.insert(tLayers, sLayerName)
  end
  Remove(tLayers, fCallback, tCallbackArgs)
end

function MarkForRemoval(vLayers)
  if type(vLayers) == "string" then
    tLayers = {vLayers}
  elseif type(vLayers) == "table" then
    tLayers = vLayers
  end
  for i, sLayerName in ipairs(tLayers) do
    sLayerName = string.lower(sLayerName)
    if _tLoadedLayers[sLayerName] ~= nil then
      _tLoadedLayers[sLayerName] = true
    end
    _tLayersToBeAdded[sLayerName] = nil
  end
end

function MarkForAddition(vLayers)
  if type(vLayers) == "string" then
    tLayers = {vLayers}
  elseif type(vLayers) == "table" then
    tLayers = vLayers
  end
  for i, sLayerName in ipairs(tLayers) do
    sLayerName = string.lower(sLayerName)
    _tLayersToBeAdded[sLayerName] = true
    if _tLoadedLayers[sLayerName] == true then
      _tLoadedLayers[sLayerName] = false
    end
  end
end

function RemoveMarkedLayers(fCallback, tCallbackArgs)
  local tLayers = {}
  for sLayerName, bMarkedForRemoval in pairs(_tLoadedLayers) do
    if bMarkedForRemoval then
      table.insert(tLayers, sLayerName)
    end
  end
  Remove(tLayers, fCallback, tCallbackArgs)
end

function ProcessMarkedLayers(fCallback, tCallbackArgs)
  local function _MarkedLayersAdded()
    _tLayersToBeAdded = nil
    
    _tLayersToBeAdded = {}
    MrxUtil.CallWithOptionalArgs(fCallback, tCallbackArgs)
  end
  
  local function _MarkedLayersRemoved()
    local tLayers = {}
    for sLayerName, _ in pairs(_tLayersToBeAdded) do
      table.insert(tLayers, sLayerName)
    end
    Add(tLayers, _MarkedLayersAdded)
  end
  
  RemoveMarkedLayers(_MarkedLayersRemoved)
end

function _AddRequest(nRequestType, vLayers, fCallback, tCallbackArgs, bCullDupes, bStatic, bClientNeedsLoadingScreen)
  local tLayers
  if type(vLayers) == "string" then
    tLayers = {vLayers}
  elseif type(vLayers) == "table" then
    tLayers = vLayers
  end
  Debug.Printf("New operation (" .. table.getn(tLayers) .. " layers)")
  local tTemp = {}
  for nLayerIndex, sLayerName in ipairs(tLayers) do
    sLayerName = string.lower(sLayerName)
    local bIgnore = false
    if Sys.GetIgnoreLayers then
      tIgnoreLayers = Sys.GetIgnoreLayers()
      for _, sIgnoreLayer in ipairs(tIgnoreLayers) do
        if sLayerName == string.lower(sIgnoreLayer) then
          Debug.Printf("Culling layer " .. sLayerName .. " from add request; the layer was found in the ignore list")
          bIgnore = true
          break
        end
      end
    end
    if not bIgnore then
      local bAssetExists = Pg.AssetExists(sLayerName, "layer")
      local bAlreadyLoaded = false
      local bAlreadyUnloaded = false
      if nRequestType == _knRequestTypeRemove and _tLoadedLayers[sLayerName] == nil and not Pg.IsStaticLayer(sLayerName) and not Pg.GetUnloadingStaticLayers() then
        bAlreadyUnloaded = true
      end
      if bCullDupes and nRequestType == _knRequestTypeAdd and (_tLoadedLayers[sLayerName] or Pg.IsStaticLayer(sLayerName)) then
        bAlreadyLoaded = true
      end
      if not bAssetExists then
        Debug.Printf("Culling layer " .. sLayerName .. " from add request; layer does not exist")
      end
      if bAlreadyLoaded then
        Debug.Printf("Culling layer " .. sLayerName .. " from add request; layer is already loaded")
      end
      if bAlreadyUnloaded then
        Debug.Printf("Culling layer " .. sLayerName .. " from remove request; layer is already unloaded")
      end
      if bAssetExists and not bAlreadyLoaded and not bAlreadyUnloaded then
        table.insert(tTemp, sLayerName)
      end
    end
  end
  tLayers = tTemp
  Debug.Printf("After culling, " .. table.getn(tLayers) .. " layers in operation")
  local nLayers = table.getn(tLayers)
  if nLayers == 0 then
    MrxUtil.CallWithOptionalArgs(fCallback, tCallbackArgs)
    return
  end
  local tRequest = {
    nDone = 0,
    nQuota = nLayers,
    fCallback = fCallback,
    tCallbackArgs = tCallbackArgs
  }
  table.insert(_tRequests, tRequest)
  tRequest.nId = table.getn(_tRequests)
  local sRequestType = "adding"
  if nRequestType == _knRequestTypeRemove then
    sRequestType = "removing"
  end
  Debug.Printf("Adding new request (" .. table.getn(_tRequests) .. "): " .. sRequestType .. " " .. nLayers .. " layers")
  for _, sLayerName in ipairs(tLayers) do
    if not _tOpQueue[sLayerName] then
      _tOpQueue[sLayerName] = {
        nOperationType = nRequestType,
        tRequests = {tRequest},
        bStatic = bStatic,
        bClientNeedsLoadingScreen = bClientNeedsLoadingScreen
      }
    else
      table.insert(_tOpQueue[sLayerName].tRequests, tRequest)
    end
    Debug.Printf("(Currently, there are " .. table.getn(_tOpQueue[sLayerName].tRequests) .. " requests for layer " .. sLayerName .. ")")
  end
  _ProcessOpQueue()
end

function _ProcessOpQueue()
  local tTemp = {}
  local nPendingOps = 0
  for sLayerName, tOp in pairs(_tOpQueue) do
    if not tOp.bMarkedForDeletion then
      tTemp[sLayerName] = tOp
      nPendingOps = nPendingOps + 1
    else
      Debug.Printf("Culling op for layer " .. sLayerName .. "!")
    end
  end
  _tOpQueue = tTemp
  Debug.Printf("(nPendingOps = " .. nPendingOps .. ")")
  if nPendingOps <= 0 then
    Sys.SetAssetRequestMax(_knOrigAssetRequestMax)
    Debug.Printf("All layer operations processed and fulfilled...resetting asset request max to " .. _knOrigAssetRequestMax)
    return
  elseif nPendingOps > Sys.GetAssetRequestMax() then
    Sys.SetAssetRequestMax(nPendingOps)
    Debug.Printf("Setting asset request max to " .. nPendingOps)
  end
  local tLoadCallbackData = {"Load"}
  local tReloadCallbackData = {"Reload"}
  local tUnloadCallbackData = {"Unload"}
  for sLayerName, tOp in pairs(_tOpQueue) do
    if _nLayersBeingProcessed >= _knLayersToProcessCap then
      return
    end
    if not tOp.bProcessed then
      if tOp.nOperationType == _knRequestTypeAdd then
        local bStatic = tOp.bStatic
        local bClientNeedsLoadingScreen = tOp.bClientNeedsLoadingScreen
        if bStatic == nil then
          bStatic = false
        end
        if bClientNeedsLoadingScreen == nil then
          bClientNeedsLoadingScreen = false
        end
        local bSuccess
        if _tLoadedLayers[sLayerName] == nil and not Pg.IsStaticLayer(sLayerName) then
          Debug.Printf("Loading " .. sLayerName .. " (static: " .. tostring(bStatic) .. ")")
          bSuccess = Pg.LoadLayer(sLayerName, not bStatic, _LayerStatusChange, tLoadCallbackData, bClientNeedsLoadingScreen)
          _nLayersBeingProcessed = _nLayersBeingProcessed + 1
          tOp.bProcessed = true
        elseif not bStatic then
          Debug.Printf("Reloading " .. sLayerName)
          bSuccess = Pg.ReloadLayer(sLayerName, _LayerStatusChange, tReloadCallbackData, bClientNeedsLoadingScreen)
          _nLayersBeingProcessed = _nLayersBeingProcessed + 1
          tOp.bProcessed = true
        end
        ASSERT(bSuccess, "Unable to load layer " .. sLayerName)
      elseif tOp.nOperationType == _knRequestTypeRemove then
        Debug.Printf("Unloading " .. sLayerName)
        local bSuccess = Pg.UnloadLayer(sLayerName, _LayerStatusChange, tUnloadCallbackData, bClientNeedsLoadingScreen)
        _nLayersBeingProcessed = _nLayersBeingProcessed + 1
        ASSERT(bSuccess, "Unable to unload layer " .. sLayerName)
        tOp.bProcessed = true
      end
    end
  end
end

function _LayerStatusChange(sRequestType, sLayerName, sLayerType, bSuccess)
  local tOp = _tOpQueue[sLayerName]
  _nLayersBeingProcessed = _nLayersBeingProcessed - 1
  if bSuccess then
    if sRequestType == "Load" or sRequestType == "Reload" then
      if _tLoadedLayers[sLayerName] == nil and not tOp.bStatic then
        _tLoadedLayers[sLayerName] = false
      end
      Debug.Printf("Request fulfilled: " .. sRequestType .. " layer " .. sLayerName .. " (static: " .. tostring(bStatic) .. ")")
    elseif sRequestType == "Unload" then
      _tLoadedLayers[sLayerName] = nil
      Debug.Printf("Request fulfilled: " .. sRequestType .. " layer " .. sLayerName)
    end
  else
    Debug.Printf(sRequestType .. " request FAILED for layer " .. sLayerName .. " MUST FIX TO MAKE QUIT TO SHELL WORK!")
    ASSERT(bSuccess, "Request " .. sRequestType .. " FAILED for layer " .. sLayerName)
  end
  local tCallbacks = {}
  if tOp then
    tOp.bMarkedForDeletion = true
    for _, tRequest in ipairs(tOp.tRequests) do
      tRequest.nDone = tRequest.nDone + 1
      Debug.Printf("Layer request " .. tRequest.nId .. ": " .. tRequest.nDone .. " of " .. tRequest.nQuota .. " complete")
      if tRequest.nDone == tRequest.nQuota then
        Debug.Printf("Layer request " .. tRequest.nId .. " completed!")
        tRequest.bFulfilled = true
      end
    end
    local tTemp = {}
    for _, tRequest in ipairs(_tRequests) do
      if tRequest.bFulfilled then
        table.insert(tCallbacks, {
          tRequest.fCallback,
          tRequest.tCallbackArgs
        })
      else
        table.insert(tTemp, tRequest)
      end
    end
    _tRequests = tTemp
  end
  _ProcessOpQueue()
  for i, tCallback in ipairs(tCallbacks) do
    MrxUtil.CallWithOptionalArgs(tCallback[1], tCallback[2])
  end
end

function SaveSingleton()
  local tSaveData = {}
  for sLayerName, bMarkedForRemoval in pairs(_tLoadedLayers) do
    if not bMarkedForRemoval then
      table.insert(tSaveData, sLayerName)
    end
  end
  for sLayerName, _ in pairs(_tLayersToBeAdded) do
    if not _tLoadedLayers[sLayerName] then
      table.insert(tSaveData, sLayerName)
    end
  end
  return tSaveData
end

function LoadSingleton(tSaveData, fCallback, tCallbackData)
  local tDynamicLayers = {}
  for sLayerName, tLayerData in pairs(_tLoadedLayers) do
    table.insert(tDynamicLayers, sLayerName)
  end
  local tRemoveLayers, tAddLayers = FindLayerIntersection(tDynamicLayers, tSaveData)
  Debug.Printf("  ----=== # dynamic layers: ", #tDynamicLayers, " save data: ", #tSaveData)
  Debug.Printf("  ----=== # culling: ", #tRemoveLayers, " adding: ", #tAddLayers)
  Remove(tRemoveLayers, function()
    Add(tAddLayers, fCallback, tCallbackData)
  end)
end

function ResetState()
  _tRequests = {}
  _tOpQueue = {}
  _tLoadedLayers = {}
  _tLayersToBeAdded = {}
end

function FindLayerIntersection(tR, tA)
  table.sort(tR)
  table.sort(tA)
  local tRemoveList = {}
  local tAddList = {}
  local iR = 1
  local iA = 1
  while tR[iR] ~= nil and tA[iA] do
    if tR[iR] == tA[iA] then
      table.insert(tAddList, tR[iR])
      iR = iR + 1
      iA = iA + 1
    elseif tR[iR] < tA[iA] then
      table.insert(tRemoveList, tR[iR])
      iR = iR + 1
    else
      table.insert(tAddList, tA[iA])
      iA = iA + 1
    end
  end
  if tR[iR] ~= nil then
    while iR <= #tR do
      table.insert(tRemoveList, tR[iR])
      iR = iR + 1
    end
  elseif tA[iA] ~= nil then
    while iA <= #tA do
      table.insert(tAddList, tA[iA])
      iA = iA + 1
    end
  end
  return tRemoveList, tAddList
end
