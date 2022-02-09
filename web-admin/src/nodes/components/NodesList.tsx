import { truncateMiddle } from "../../utils/capitalize";
import SearchableTable from "../../components/tables/SearchableTable";
import getNodes from "../queries/getNodes";
import { ClipboardCopyIcon, PlusCircleIcon } from "@heroicons/react/outline";
import copy from "copy-to-clipboard";
import { useState } from "react";
import StartNodeForm from "../components/StartNodeForm";
import { useModal } from "../../contexts/modal";
import { PlayIcon, StopIcon } from "@heroicons/react/outline";
import { useConfirm } from "../../contexts/confirm";
import adminStopNode from "../mutations/adminStopNode";
import { useQueryClient } from "react-query";
import { Link } from "react-router-dom";
import { Node } from "@l2-technology/sensei-client"

const SimpleColumn = ({ value, className }) => {
  return (
    <td
      className={`px-6 py-4 whitespace-nowrap text-sm leading-5 font-medium text-light-plum ${className}`}
    >
      {value}
    </td>
  );
};

const ActionsColumn = ({ value, node, className }) => {
  const { showModal, hideModal } = useModal();
  const { showConfirm } = useConfirm();
  const queryClient = useQueryClient();

  const nodeStarted = () => {
    queryClient.invalidateQueries("nodes");
    hideModal();
  };

  const startNodeClicked = async () => {
    showModal({
      component: <StartNodeForm pubkey={node.pubkey} callback={nodeStarted} />,
    });
  };

  const stopNodeClicked = () => {
    showConfirm({
      title: "Are you sure you want to stop this node?",
      description:
        "A stopped node can no longer send, receive, or route payments.  The node will also no longer be monitoring the chain for misbehavior.",
      ctaText: "Yes, stop it",
      callback: async () => {
        await adminStopNode(node.pubkey);
        queryClient.invalidateQueries("nodes");
      },
    });
  };

  return (
    <td
      className={`px-6 py-4 whitespace-nowrap text-sm leading-5 font-medium text-light-plum ${className}`}
    >
      {node.status === "Stopped" && (
        <PlayIcon
          className="inline-block h-6 cursor-pointer"
          onClick={startNodeClicked}
        />
      )}
      {node.status === "Running" && (
        <StopIcon
          className="inline-block h-6 cursor-pointer"
          onClick={stopNodeClicked}
        />
      )}

      <Link
        to={`/admin/channels/open?connection=${node.pubkey}@127.0.0.1:${node.listenPort}`}
      >
        <PlusCircleIcon className="inline-block h-6 cursor-pointer" />
      </Link>
    </td>
  );
};

const StatusColumn = ({ value, className }) => {
  return (
    <td
      className={`px-6 py-4 whitespace-nowrap text-sm leading-5 font-medium text-light-plum ${className}`}
    >
      {value === "Stopped" && (
        <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-red-200 text-red-800">
          Stopped
        </span>
      )}
      {value === "Running" && (
        <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-green-100 text-green-800">
          Running
        </span>
      )}
    </td>
  );
};

const ConnectionInfoColumn = ({ node, value, className }) => {
  let [copied, setCopied] = useState(false);

  const copyClicked = () => {
    copy(`${node.pubkey}@${node.listenAddr}:${node.listenPort}`);
    setCopied(true);
    setTimeout(() => {
      setCopied(false);
    }, 1000);
  };

  return copied ? (
    <td
      className={`px-6 py-4 whitespace-nowrap text-sm leading-5 font-medium text-light-plum ${className}`}
    >
      Copied! &nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;
      &nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;
      &nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;
      &nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;
      &nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;
      &nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;
    </td>
  ) : (
    <td
      onClick={copyClicked}
      className={`group cursor-pointer px-6 py-4 whitespace-nowrap text-sm leading-5 font-medium text-light-plum ${className}`}
    >
      {value}{" "}
      <span className="inline-block group-hover:hidden">
        &nbsp;&nbsp;&nbsp;&nbsp;
      </span>
      <ClipboardCopyIcon className="w-4 text-gray-500 hidden group-hover:inline-block" />
    </td>
  );
};

const NodeRow = ({ result, extraClass, attributes }) => {
  let columnKeyComponentMap = {
    status: StatusColumn,
    connectionInfo: ConnectionInfoColumn,
    actions: ActionsColumn,
  };

  return (
    <tr className={`border-b border-plum-200 ${extraClass}`}>
      {attributes.map(({ key, label, className }) => {
        let value = result[key];
        let ColumnComponent = columnKeyComponentMap[key]
          ? columnKeyComponentMap[key]
          : SimpleColumn;

        return (
          <ColumnComponent
            key={key}
            node={result}
            value={value}
            className={className}
          />
        );
      })}
    </tr>
  );
};

const NodesListCard = () => {
  const emptyTableHeadline = "No nodes found";
  const emptyTableSubtext = "Try changing the search term";
  const searchBarPlaceholder = "Search";

  const attributes = [
    {
      key: "username",
      label: "Username",
    },
    {
      key: "alias",
      label: "Alias",
    },
    {
      key: "role",
      label: "Role",
    },
    {
      key: "connectionInfo",
      label: "Connection Info",
    },
    {
      key: "status",
      label: "Status",
    },
    {
      key: "actions",
      label: "Actions",
    },
  ];

  const transformResults = (nodes: Node[]) => {
    return nodes.map((node) => {
      return {
        ...node,
        role: node.role === 0 ? "Sensei" : "Child",
        connectionInfo: `${truncateMiddle(node.pubkey, 10)}@${
          "127.0.0.1"
        }:${node.listenPort}`,
        status: node.status === 0 ? "Stopped" : "Running",
        actions: "Action",
      };
    });
  };

  const queryFunction = async ({ queryKey }) => {
    const [_key, { page, searchTerm, take }] = queryKey;
    const response = await getNodes({ page, searchTerm, take });
    return {
      results: transformResults(response.nodes),
      hasMore: response.pagination.hasMore,
      total: response.pagination.total
    }
  };

  return (
    <SearchableTable
      attributes={attributes}
      queryKey="nodes"
      queryFunction={queryFunction}
      emptyTableHeadline={emptyTableHeadline}
      emptyTableSubtext={emptyTableSubtext}
      searchBarPlaceholder={searchBarPlaceholder}
      hasHeader
      itemsPerPage={5}
      RowComponent={NodeRow}
    />
  );
};

export default NodesListCard;