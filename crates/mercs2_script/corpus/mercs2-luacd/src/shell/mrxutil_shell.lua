function CallWithOptionalArgs(fFunction, tArgs)
  if type(fFunction) == "function" then
    if type(tArgs) == "table" then
      return fFunction(unpack(tArgs))
    else
      return fFunction()
    end
  end
end

function ProcessCallbackTable(tCallbacks, tAdditionalArgs)
  if type(tCallbacks) == "table" then
    for _, tCallback in ipairs(tCallbacks) do
      local tCallbackArgs = tCallback[2]
      local tArgs
      local bIsCallbackTable = type(tCallbackArgs) == "table"
      local bIsAdditionalTable = type(tAdditionalArgs) == "table"
      if bIsCallbackTable and bIsAdditionalTable then
        tArgs = MergeIndexedTables(tCallbackArgs, tAdditionalArgs)
      elseif bIsCallbackTable then
        tArgs = tCallbackArgs
      elseif bIsAdditionalTable then
        tArgs = tAdditionalArgs
      end
      CallWithOptionalArgs(tCallback[1], tArgs)
    end
  end
end
