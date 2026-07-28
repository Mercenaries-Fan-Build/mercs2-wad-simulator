function QueryRepair(intVal)
  if intVal == 1 then
    return "MakeUpright"
  end
  return ""
end

function QueryActiveUse(intVal)
  if intVal == 1 then
    return "SuperUse"
  end
  return ""
end

function SuperUse(floatval, aiguid)
end

function Use(aiguid, floatval)
end

function MakeUpright(objectguid, aiguid)
end
