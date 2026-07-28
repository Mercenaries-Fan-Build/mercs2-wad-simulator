function Goal(tParameters)
  if tParameters.AIGuid then
    Event.Create(Event.ObjectHibernation, {
      tParameters.AIGuid,
      
      "awake"
    }, function()
      Ai.Goal(tParameters)
    end)
  end
end

function DefaultGoal(tParameters)
  if tParameters.AIGuid then
    Event.Create(Event.ObjectHibernation, {
      tParameters.AIGuid,
      "awake"
    }, function()
      Ai.DefaultGoal(tParameters)
    end)
  end
end

function RemoveGoal(tParameters)
  if tParameters.AIGuid then
    Event.Create(Event.ObjectHibernation, {
      tParameters.AIGuid,
      "awake"
    }, function()
      Ai.RemoveGoal(tParameters)
    end)
  end
end

function Deploy(tParameters)
  if tParameters.Vehicle then
    Event.Create(Event.ObjectHibernation, {
      tParameters.AIGuid,
      "awake"
    }, function()
      Ai.Deploy(tParameters)
    end)
  end
end

function Role(tParameters)
  if tParameters.AIGuid then
    Event.Create(Event.ObjectHibernation, {
      tParameters.AIGuid,
      "awake"
    }, function()
      Ai.Role(tParameters)
    end)
  end
end
